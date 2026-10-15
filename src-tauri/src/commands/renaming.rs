//! Renommer des photos avec un modèle : aperçu, renommage
//! (Cmd+Z) et modèles enregistrés.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::catalog_info::CatalogReadFailure;
use super::edits::{EditFailure, failure, lock, media_ids_of};
use super::media_move::{FileMoveRefusalDto, FileMoveResultDto, run};
use crate::catalog::repositories::rename_templates;
use crate::catalog::{Catalog, CatalogError};
use crate::edits::History;
use crate::renaming::{self, RenameTemplate};

/// Noms montrés dans l'aperçu.
const EXAMPLES: usize = 20;

/// Un nom avant et après.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameExample {
    /// Nom actuel.
    pub from: String,
    /// Nouveau nom.
    pub to: String,
}

/// Aperçu d'un renommage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreviewDto {
    /// Premiers noms, avant et après.
    pub examples: Vec<RenameExample>,
    /// Photos et vidéos renommées (l'autre fichier des paires compris).
    pub media_count: u32,
    /// Fichiers annexes renommés avec elles.
    pub sidecar_count: u32,
    /// Raison d'un refus.
    pub refusal: Option<FileMoveRefusalDto>,
}

/// Modèle enregistré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameTemplateDto {
    /// Identifiant.
    #[specta(type = u32)]
    pub id: i64,
    /// Nom.
    pub name: String,
    /// Modèle.
    pub template: RenameTemplate,
}

fn small(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn unavailable(error: &CatalogError) -> CatalogReadFailure {
    tracing::error!(%error, "cannot prepare the renaming");
    CatalogReadFailure::CatalogUnavailable
}

/// Aperçu du renommage des photos avec `template`.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_rename_preview(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    template: RenameTemplate,
) -> Result<RenamePreviewDto, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let plan = renaming::plan_rename(catalog.connection(), &media_ids_of(media_ids), &template)
        .map_err(|error| unavailable(&error))?;
    Ok(match plan {
        Ok(plan) => RenamePreviewDto {
            examples: plan
                .groups
                .iter()
                .flat_map(|group| group.files.iter().take(group.media.len()))
                .take(EXAMPLES)
                .map(|(from, to)| RenameExample {
                    from: name_of(from),
                    to: name_of(to),
                })
                .collect(),
            media_count: small(plan.media_count()),
            sidecar_count: small(plan.sidecar_count()),
            refusal: None,
        },
        Err(refusal) => RenamePreviewDto {
            examples: Vec::new(),
            media_count: 0,
            sidecar_count: 0,
            refusal: Some(refusal.into()),
        },
    })
}

/// Renomme les photos avec `template`, avec l'autre fichier de leur paire
/// et leurs fichiers annexes ; Cmd+Z leur rend leur nom.
///
/// # Errors
///
/// Voir [`EditFailure`].
#[tauri::command]
#[specta::specta]
pub async fn rename_media(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    template: RenameTemplate,
) -> Result<FileMoveResultDto, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let plan = renaming::plan_rename(catalog.connection(), &media_ids_of(media_ids), &template)
        .map_err(|error| failure(&error))?;
    match plan {
        Ok(plan) => run(&mut history, &mut catalog, plan).map_err(|error| failure(&error)),
        Err(refusal) => Ok(FileMoveResultDto::refused(refusal)),
    }
}

/// Modèles de renommage enregistrés (un modèle illisible est ignoré et
/// journalisé).
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_rename_templates(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<RenameTemplateDto>, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let stored =
        rename_templates::list(catalog.connection()).map_err(|error| unavailable(&error))?;
    Ok(stored
        .into_iter()
        .filter_map(|stored| {
            serde_json::from_str(&stored.content)
                .inspect_err(|error| tracing::warn!(%error, "unreadable rename template"))
                .ok()
                .map(|template| RenameTemplateDto {
                    id: stored.id,
                    name: stored.name,
                    template,
                })
        })
        .collect())
}

/// Enregistre un modèle sous `name` (remplace celui du même nom).
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue ne peut pas
/// être écrit.
#[tauri::command]
#[specta::specta]
pub async fn save_rename_template(
    catalog: State<'_, Mutex<Catalog>>,
    name: String,
    template: RenameTemplate,
) -> Result<(), CatalogReadFailure> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    let content = serde_json::to_string(&template).map_err(|error| {
        tracing::error!(%error, "cannot serialize a rename template");
        CatalogReadFailure::CatalogUnavailable
    })?;
    let catalog = lock(&catalog);
    rename_templates::put(catalog.connection(), name, &content).map_err(|error| unavailable(&error))
}

/// Supprime un modèle enregistré.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue ne peut pas
/// être écrit.
#[tauri::command]
#[specta::specta]
pub async fn delete_rename_template(
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
) -> Result<(), CatalogReadFailure> {
    let catalog = lock(&catalog);
    rename_templates::delete(catalog.connection(), i64::from(id))
        .map_err(|error| unavailable(&error))
}
