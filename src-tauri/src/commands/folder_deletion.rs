//! Supprimer un dossier depuis le panneau Dossiers : résumé, « Retirer du catalogue » (annulable) ou « Placer dans la
//! corbeille », refusée si le dossier contient des fichiers inconnus du
//! catalogue.

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::catalog_info::CatalogReadFailure;
use super::edits::{EditFailure, EditOutcome, failure, lock, outcome};
use crate::catalog::Catalog;
use crate::catalog::repositories::folders;
use crate::deletion::{FolderTrashOutcome, trash_folder as trash_folder_on_disk};
use crate::domain::{FolderId, MediaId};
use crate::edits::{Edit, History};
use crate::files::{SystemTrash, folder_inventory};
use crate::folders::{TrashCheck, folder_contents, trash_check};
use crate::thumbnails::{PreviewCache, ThumbnailCache};
use crate::workspace::Workspace;

/// Mise à la corbeille possible, ou pourquoi elle ne l'est pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrashCheckDto {
    /// Tous les fichiers sont connus du catalogue.
    Possible,
    /// Dossier introuvable (disque débranché, dossier déplacé).
    Offline,
    /// Un dossier n'a pas pu être lu.
    Unreadable,
    /// Des fichiers sont inconnus du catalogue.
    #[serde(rename_all = "camelCase")]
    UnknownFiles {
        /// Les premiers, chemin relatif au dossier supprimé.
        examples: Vec<String>,
        /// Nombre total.
        count: u32,
    },
}

/// Ce que contient le dossier à supprimer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderDeletionDto {
    /// Nom du dossier.
    pub name: String,
    /// Chemin complet.
    pub path: String,
    /// Sous-dossiers, à tous les niveaux.
    pub subfolders: u32,
    /// Photos, sous-dossiers compris.
    pub photos: u32,
    /// Vidéos, sous-dossiers compris.
    pub videos: u32,
    /// Taille des photos et vidéos, en octets : déclarée `u32` pour
    /// TypeScript (`number`), envoyée exacte (voir `FolderAnalysisSummary`).
    #[specta(type = u32)]
    pub size_bytes: u64,
    /// Corbeille possible ou non.
    pub trash: TrashCheckDto,
}

/// Résultat de « Placer dans la corbeille ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderTrashResultDto {
    /// Dans la corbeille ; ses médias ont quitté le catalogue.
    Trashed {
        /// Médias retirés.
        count: u32,
    },
    /// Refusé avant de rien faire.
    Refused {
        /// Raison.
        check: TrashCheckDto,
    },
    /// Le dossier n'est plus au catalogue.
    Unknown,
    /// Refusé par le système : le dossier est resté à sa place.
    Failed {
        /// Raison donnée par le système.
        reason: String,
    },
}

/// Résumé du dossier à supprimer ; `null` s'il n'est plus au catalogue.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_folder_deletion(
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<Option<FolderDeletionDto>, CatalogReadFailure> {
    let contents = {
        let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
        folder_contents(catalog.connection(), folder_id_of(folder_id))
    }
    .map_err(|error| {
        tracing::error!(%error, "cannot read the folder to delete");
        CatalogReadFailure::CatalogUnavailable
    })?;
    let Some(contents) = contents else {
        return Ok(None);
    };
    // Le disque est lu sans tenir le verrou du catalogue.
    let path = &contents.folder.path;
    let inventory = path.is_dir().then(|| folder_inventory(path));
    let disk_subfolders = inventory
        .as_ref()
        .and_then(|inventory| inventory.as_ref().ok())
        .map(|inventory| inventory.folders);
    let check = trash_check(&contents, inventory);
    Ok(Some(FolderDeletionDto {
        name: path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        path: path.display().to_string(),
        subfolders: count_dto(disk_subfolders.unwrap_or(contents.subfolders)),
        photos: count_dto(contents.photos()),
        videos: count_dto(contents.videos()),
        size_bytes: contents.size(),
        trash: check_dto(&check, path),
    }))
}

/// Retire du catalogue le dossier, ses sous-dossiers et leurs médias ; les
/// fichiers restent sur le disque. Annulable (Cmd+Z).
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn remove_folder_from_catalog(
    app: AppHandle,
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<EditOutcome, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let ids: Vec<MediaId> = folders::media_in_branch(catalog.connection(), folder_id_of(folder_id))
        .map_err(|error| failure(&error))?
        .into_iter()
        .map(|media| media.id)
        .collect();
    let summary = Edit::removal(&catalog, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    // La vignette est gardée pour une éventuelle annulation.
    if let Some(previews) = app.try_state::<PreviewCache>() {
        previews.forget(&ids);
    }
    Ok(outcome(Some(summary), &history))
}

/// Place le dossier dans la corbeille du système, s'il ne contient que des
/// fichiers connus du catalogue ; ses médias quittent le catalogue. Pas
/// d'annulation par Cmd+Z : le dossier se récupère dans la corbeille.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn trash_folder(
    app: AppHandle,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<FolderTrashResultDto, EditFailure> {
    let id = folder_id_of(folder_id);
    let path = {
        let catalog = lock(&catalog);
        folders::list(catalog.connection())
            .map_err(|error| failure(&error))?
            .into_iter()
            .find(|folder| folder.id == id)
            .map(|folder| folder.path)
    };
    let result =
        trash_folder_on_disk(&catalog, &SystemTrash, id).map_err(|error| failure(&error))?;
    Ok(match result {
        FolderTrashOutcome::Trashed { removed } => {
            // Ces médias ne reviendront pas : leurs copies réduites sont
            // effacées.
            if let Some(workspace) = app.try_state::<Workspace>() {
                let thumbnails = ThumbnailCache::new(workspace.thumbnails_dir());
                for &media_id in &removed {
                    thumbnails.remove(media_id);
                }
            }
            if let Some(previews) = app.try_state::<PreviewCache>() {
                previews.forget(&removed);
            }
            FolderTrashResultDto::Trashed {
                count: count_dto(removed.len() as u64),
            }
        }
        FolderTrashOutcome::Refused(check) => FolderTrashResultDto::Refused {
            check: check_dto(&check, path.as_deref().unwrap_or(std::path::Path::new(""))),
        },
        FolderTrashOutcome::Unknown => FolderTrashResultDto::Unknown,
        FolderTrashOutcome::Failed(reason) => FolderTrashResultDto::Failed { reason },
    })
}

fn check_dto(check: &TrashCheck, folder: &std::path::Path) -> TrashCheckDto {
    match check {
        TrashCheck::Possible => TrashCheckDto::Possible,
        TrashCheck::Offline => TrashCheckDto::Offline,
        TrashCheck::Unreadable => TrashCheckDto::Unreadable,
        TrashCheck::UnknownFiles { examples, count } => TrashCheckDto::UnknownFiles {
            examples: examples
                .iter()
                .map(|path| {
                    path.strip_prefix(folder)
                        .unwrap_or(path)
                        .display()
                        .to_string()
                })
                .collect(),
            count: count_dto(*count),
        },
    }
}

fn folder_id_of(id: u32) -> FolderId {
    FolderId::new(i64::from(id))
}

fn count_dto(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}
