//! Journal des opérations sur les fichiers, consultable :
//! Bibliothèque ▸ « Journal des opérations… ».

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::catalog_info::CatalogReadFailure;
use super::edits::lock;
use crate::catalog::Catalog;
use crate::catalog::repositories::operations_log;

/// Opérations montrées.
const OPERATIONS: u32 = 300;

/// Fichiers montrés pour une opération.
const ITEMS: u32 = 500;

/// Nature d'une opération.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OperationKindDto {
    /// Mise à la corbeille.
    Trash,
    /// Copie vérifiée (import par copie).
    Copy,
    /// Déplacement ou renommage.
    Move,
    /// Copie de sauvegarde (import).
    Backup,
    /// Inconnue (version plus récente).
    Other,
}

/// État d'une opération.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatusDto {
    /// En cours.
    Running,
    /// Terminée.
    Completed,
    /// Arrêtée par l'utilisateur.
    Cancelled,
    /// Interrompue par une erreur, ou en partie refusée.
    Failed,
}

/// Résultat pour un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ItemResultDto {
    /// En attente.
    Pending,
    /// Fait.
    Done,
    /// Introuvable.
    Missing,
    /// En échec.
    Failed,
}

/// Une opération du journal.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationDto {
    /// Identifiant.
    #[specta(type = u32)]
    pub id: i64,
    /// Nature.
    pub kind: OperationKindDto,
    /// Début, en millisecondes depuis 1970 (UTC) ; un nombre exact côté
    /// interface jusqu'à 2⁵³ (le type `f64` deviendrait `number | null`).
    #[specta(type = u32)]
    pub started_at: i64,
    /// État.
    pub status: OperationStatusDto,
    /// Fichiers traités.
    #[specta(type = u32)]
    pub done: u64,
    /// Fichiers introuvables.
    #[specta(type = u32)]
    pub missing: u64,
    /// Fichiers en échec.
    #[specta(type = u32)]
    pub failed: u64,
    /// Fichiers restés en attente.
    #[specta(type = u32)]
    pub pending: u64,
}

/// Un fichier d'une opération.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationItemDto {
    /// Chemin d'origine.
    pub source: String,
    /// Chemin d'arrivée, s'il est connu.
    pub destination: Option<String>,
    /// Résultat.
    pub result: ItemResultDto,
    /// Raison d'un échec.
    pub detail: Option<String>,
}

fn kind_of(text: &str) -> OperationKindDto {
    match text {
        "trash" => OperationKindDto::Trash,
        "copy" => OperationKindDto::Copy,
        "move" => OperationKindDto::Move,
        "backup" => OperationKindDto::Backup,
        _ => OperationKindDto::Other,
    }
}

fn status_of(text: &str) -> OperationStatusDto {
    match text {
        "running" => OperationStatusDto::Running,
        "completed" => OperationStatusDto::Completed,
        "cancelled" => OperationStatusDto::Cancelled,
        _ => OperationStatusDto::Failed,
    }
}

fn result_of(text: &str) -> ItemResultDto {
    match text {
        "pending" => ItemResultDto::Pending,
        "done" => ItemResultDto::Done,
        "missing" => ItemResultDto::Missing,
        _ => ItemResultDto::Failed,
    }
}

fn unavailable(error: &crate::catalog::CatalogError) -> CatalogReadFailure {
    tracing::error!(%error, "cannot read the operations journal");
    CatalogReadFailure::CatalogUnavailable
}

/// Les dernières opérations, la plus récente d'abord.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_operations(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<OperationDto>, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let records = operations_log::recent(catalog.connection(), OPERATIONS)
        .map_err(|error| unavailable(&error))?;
    Ok(records
        .into_iter()
        .map(|record| OperationDto {
            id: record.id,
            kind: kind_of(&record.kind),
            started_at: record.started_at,
            status: status_of(&record.status),
            done: record.done,
            missing: record.missing,
            failed: record.failed,
            pending: record.pending,
        })
        .collect())
}

/// Les premiers fichiers d'une opération.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_operation_items(
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
) -> Result<Vec<OperationItemDto>, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let records = operations_log::items(catalog.connection(), i64::from(id), ITEMS)
        .map_err(|error| unavailable(&error))?;
    Ok(records
        .into_iter()
        .map(|record| OperationItemDto {
            result: result_of(&record.result),
            source: record.source,
            destination: record.destination,
            detail: record.detail,
        })
        .collect())
}
