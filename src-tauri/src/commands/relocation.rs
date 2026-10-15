//! Relocaliser un fichier ou un dossier : choisir le nouvel
//! emplacement, voir ce qui y est retrouvé, puis relocaliser (Cmd+Z).

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use super::dialogs;
use super::edits::{EditOutcome, lock, outcome};
use crate::catalog::Catalog;
use crate::catalog::repositories::media;
use crate::domain::{FolderId, MediaId};
use crate::edits::History;
use crate::relocation::{self, RelocationError};

/// Relocalisation refusée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RelocationFailure {
    /// Le média, le dossier ou l'emplacement choisi est introuvable.
    NotFound,
    /// Le fichier choisi n'est pas du même type que la photo.
    WrongFormat,
    /// Le fichier ou le dossier choisi est déjà au catalogue.
    AlreadyInCatalog,
    /// Le dossier choisi est le dossier lui-même ou l'un de ses
    /// sous-dossiers.
    IntoItself,
    /// La fenêtre de sélection a renvoyé un emplacement inutilisable.
    UnsupportedLocation,
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
}

impl From<RelocationError> for RelocationFailure {
    fn from(error: RelocationError) -> Self {
        match error {
            RelocationError::NotFound => Self::NotFound,
            RelocationError::WrongFormat => Self::WrongFormat,
            RelocationError::AlreadyInCatalog => Self::AlreadyInCatalog,
            RelocationError::IntoItself => Self::IntoItself,
            RelocationError::Catalog(error) => {
                tracing::error!(%error, "cannot relocate");
                Self::CatalogUnavailable
            }
        }
    }
}

/// Ce que donnerait la relocalisation d'un dossier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderRelocationPreview {
    /// Photos et vidéos du dossier et de ses sous-dossiers.
    pub total: u32,
    /// Celles retrouvées au nouvel emplacement.
    pub found: u32,
}

fn small(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Fait choisir le nouveau fichier d'une photo, en partant du dossier où
/// elle était (s'il existe encore) ; `null` si la fenêtre est fermée.
///
/// # Errors
///
/// Voir [`RelocationFailure`].
#[tauri::command]
#[specta::specta]
pub async fn choose_relocated_file(
    app: AppHandle,
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
    title: String,
    filter_name: String,
) -> Result<Option<String>, RelocationFailure> {
    let start = {
        let catalog = lock(&catalog);
        media::read_with_paths(catalog.connection(), &[MediaId::new(i64::from(media_id))])
            .map_err(|error| RelocationFailure::from(RelocationError::Catalog(error)))?
            .into_iter()
            .next()
            .and_then(|(_, path)| path.parent().map(Path::to_path_buf))
            .filter(|folder| folder.is_dir())
    };
    let chosen = dialogs::pick_media_file(&app, title, &filter_name, start)
        .map_err(|_| RelocationFailure::UnsupportedLocation)?;
    Ok(chosen.map(|path| path.display().to_string()))
}

/// Fait choisir le nouvel emplacement d'un dossier ; `null` si la fenêtre
/// est fermée.
///
/// # Errors
///
/// [`RelocationFailure::UnsupportedLocation`] si l'emplacement n'est pas un
/// chemin.
#[tauri::command]
#[specta::specta]
pub async fn choose_relocated_folder(
    app: AppHandle,
    title: String,
) -> Result<Option<String>, RelocationFailure> {
    let chosen = dialogs::pick_folder(&app, title, None)
        .map_err(|_| RelocationFailure::UnsupportedLocation)?;
    Ok(chosen.map(|path| path.display().to_string()))
}

/// La photo se trouve désormais à `path` (annulable).
///
/// # Errors
///
/// Voir [`RelocationFailure`] ; rien n'est changé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn relocate_media(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
    path: String,
) -> Result<EditOutcome, RelocationFailure> {
    let path = dialogs::checked_path(&path).map_err(|_| RelocationFailure::UnsupportedLocation)?;
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = relocation::relocate_file(
        &mut catalog,
        &mut history,
        MediaId::new(i64::from(media_id)),
        &path,
    )?;
    Ok(outcome(Some(summary), &history))
}

/// Ce que donnerait la relocalisation d'un dossier en `path`.
///
/// # Errors
///
/// Voir [`RelocationFailure`].
#[tauri::command]
#[specta::specta]
pub async fn get_folder_relocation(
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
    path: String,
) -> Result<FolderRelocationPreview, RelocationFailure> {
    let path = dialogs::checked_path(&path).map_err(|_| RelocationFailure::UnsupportedLocation)?;
    let catalog = lock(&catalog);
    let plan = relocation::plan_folder(
        catalog.connection(),
        FolderId::new(i64::from(folder_id)),
        &path,
    )?;
    Ok(FolderRelocationPreview {
        total: small(plan.total()),
        found: small(plan.found()),
    })
}

/// Le dossier se trouve désormais en `path` (annulable).
///
/// # Errors
///
/// Voir [`RelocationFailure`] ; rien n'est changé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn relocate_folder(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
    path: String,
) -> Result<EditOutcome, RelocationFailure> {
    let path = dialogs::checked_path(&path).map_err(|_| RelocationFailure::UnsupportedLocation)?;
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = relocation::relocate_folder(
        &mut catalog,
        &mut history,
        FolderId::new(i64::from(folder_id)),
        &path,
    )?;
    Ok(outcome(Some(summary), &history))
}
