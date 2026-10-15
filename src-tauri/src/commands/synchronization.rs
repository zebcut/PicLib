//! Synchroniser un dossier : la comparaison du disque et du
//! catalogue, puis l'import des nouveaux fichiers ; retirer les disparus et
//! relire les modifiés passent par les commandes habituelles.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use super::edits::lock;
use super::import::{ImportStartFailure, ImportStarted, start_reference_import};
use super::jobs::AppJobManager;
use crate::catalog::Catalog;
use crate::catalog::repositories::{folders, import_preferences};
use crate::domain::FolderId;
use crate::jobs::CancellationToken;
use crate::synchronization::{self, SyncError};

/// Noms montrés par catégorie.
const EXAMPLES: usize = 20;

/// Différences entre le disque et le catalogue, pour les confirmer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderSyncDto {
    /// Fichiers nouveaux.
    pub new_count: u32,
    /// Leurs premiers noms.
    pub new_examples: Vec<String>,
    /// Médias dont le fichier a disparu.
    pub missing_ids: Vec<u32>,
    /// Leurs premiers noms.
    pub missing_examples: Vec<String>,
    /// Médias dont le fichier a changé.
    pub modified_ids: Vec<u32>,
    /// Leurs premiers noms.
    pub modified_examples: Vec<String>,
}

/// Comparaison impossible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyncFailure {
    /// Le dossier n'est plus au catalogue.
    NotFound,
    /// Le dossier est introuvable ou illisible sur le disque.
    Unreadable,
    /// Le catalogue n'est pas ouvert ou est illisible.
    CatalogUnavailable,
}

fn names<'a>(paths: impl Iterator<Item = &'a std::path::PathBuf>) -> Vec<String> {
    paths
        .take(EXAMPLES)
        .map(|path| {
            path.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        })
        .collect()
}

fn small(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Compare le dossier et son contenu sur le disque, sous-dossiers compris.
///
/// # Errors
///
/// Voir [`SyncFailure`].
#[tauri::command]
#[specta::specta]
pub async fn get_folder_sync(
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<FolderSyncDto, SyncFailure> {
    // Les XMP modifiés par un autre logiciel sont repérés au passage
    //.
    if let Err(error) = crate::xmp_files::check_folder(&catalog, i64::from(folder_id)) {
        tracing::warn!(%error, "cannot check the folder's XMP files");
    }
    let differences = synchronization::compare(
        &catalog,
        FolderId::new(i64::from(folder_id)),
        &CancellationToken::default(),
    )
    .map_err(|error| match error {
        SyncError::NotFound => SyncFailure::NotFound,
        SyncError::Scan(error) => {
            tracing::warn!(%error, "cannot read the folder to synchronize");
            SyncFailure::Unreadable
        }
        SyncError::Catalog(error) => {
            tracing::error!(%error, "cannot compare the folder");
            SyncFailure::CatalogUnavailable
        }
    })?;
    Ok(FolderSyncDto {
        new_count: u32::try_from(differences.new_files.len()).unwrap_or(u32::MAX),
        new_examples: names(differences.new_files.iter()),
        missing_ids: differences
            .missing
            .iter()
            .map(|(id, _)| small(id.get()))
            .collect(),
        missing_examples: names(differences.missing.iter().map(|(_, path)| path)),
        modified_ids: differences
            .modified
            .iter()
            .map(|(id, _)| small(id.get()))
            .collect(),
        modified_examples: names(differences.modified.iter().map(|(_, path)| path)),
    })
}

/// Importe par référence les nouveaux fichiers du dossier (ceux déjà au
/// catalogue sont ignorés).
///
/// # Errors
///
/// Voir [`ImportStartFailure`].
#[tauri::command]
#[specta::specta]
pub async fn import_new_files(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<ImportStarted, ImportStartFailure> {
    let (folder, stacking) = {
        let catalog = lock(&catalog);
        let connection = catalog.connection();
        let read = || -> Result<_, crate::catalog::CatalogError> {
            let folder = folders::list(connection)?
                .into_iter()
                .find(|folder| folder.id.get() == i64::from(folder_id));
            Ok((folder, import_preferences::read(connection)?.stacking))
        };
        let (folder, stacking) = read().map_err(|error| {
            tracing::error!(%error, "cannot read the folder to import");
            ImportStartFailure::CatalogUnavailable
        })?;
        (
            folder.ok_or(ImportStartFailure::CatalogUnavailable)?.path,
            stacking,
        )
    };
    start_reference_import(app, &jobs, folder, stacking)
}
