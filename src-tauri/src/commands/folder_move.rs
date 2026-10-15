//! Déplacer un dossier depuis le panneau Dossiers :
//! résumé avant confirmation, puis déplacement annulable (Cmd+Z).

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::catalog_info::CatalogReadFailure;
use super::edits::{EditFailure, EditOutcome, failure, lock, outcome};
use crate::catalog::Catalog;
use crate::domain::FolderId;
use crate::edits::History;
use crate::moving::{MoveOutcome, MoveRefusal, check_move, move_folder as move_folder_in};

/// Raison pour laquelle un dossier ne peut pas être déplacé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MoveRefusalDto {
    /// Déjà dans la destination.
    AlreadyThere,
    /// Dans lui-même ou l'un de ses sous-dossiers.
    IntoItself,
    /// Dossier introuvable.
    Missing,
    /// Destination introuvable.
    DestinationMissing,
    /// Destination sur un autre disque.
    OtherDisk,
    /// Sur une carte mémoire.
    CameraCard,
}

/// Résumé d'un déplacement, avant confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderMoveDto {
    /// Nom du dossier.
    pub name: String,
    /// Chemin du dossier.
    pub path: String,
    /// Nom de la destination.
    pub destination_name: String,
    /// Chemin de la destination.
    pub destination_path: String,
    /// Nom que prendra le dossier (suffixé `-2`… si le nom est pris) ;
    /// `null` si le déplacement est refusé.
    pub target_name: Option<String>,
    /// Photos, sous-dossiers compris.
    pub photos: u32,
    /// Vidéos, sous-dossiers compris.
    pub videos: u32,
    /// Sous-dossiers au catalogue, à tous les niveaux.
    pub subfolders: u32,
    /// Raison du refus, s'il y en a une.
    pub refusal: Option<MoveRefusalDto>,
}

/// Résultat d'un déplacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderMoveResultDto {
    /// Déplacé ; annulable.
    Moved {
        /// Modification faite et historique.
        outcome: EditOutcome,
    },
    /// Refusé avant de rien faire.
    Refused {
        /// Raison.
        refusal: MoveRefusalDto,
    },
    /// Le dossier ou la destination n'est plus au catalogue.
    Unknown,
    /// Refusé par le système : le dossier est resté à sa place.
    Failed {
        /// Raison donnée par le système.
        reason: String,
    },
}

/// Résumé du déplacement du dossier `folder_id` dans `destination_id` ;
/// `null` si l'un des deux n'est plus au catalogue.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_folder_move(
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
    destination_id: u32,
) -> Result<Option<FolderMoveDto>, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let check = check_move(
        catalog.connection(),
        folder_id_of(folder_id),
        folder_id_of(destination_id),
    )
    .map_err(|error| {
        tracing::error!(%error, "cannot check the folder move");
        CatalogReadFailure::CatalogUnavailable
    })?;
    Ok(check.map(|check| FolderMoveDto {
        name: name_of(&check.contents.folder.path),
        path: check.contents.folder.path.display().to_string(),
        destination_name: name_of(&check.destination.path),
        destination_path: check.destination.path.display().to_string(),
        target_name: check.target.as_ref().ok().map(|target| name_of(target)),
        photos: count_dto(check.contents.photos()),
        videos: count_dto(check.contents.videos()),
        subfolders: count_dto(check.contents.subfolders),
        refusal: check.target.err().map(MoveRefusalDto::from),
    }))
}

/// Déplace le dossier `folder_id` dans `destination_id`, sur le même
/// disque, de façon annulable (Cmd+Z le remet à sa place).
///
/// # Errors
///
/// Voir [`EditFailure`].
#[tauri::command]
#[specta::specta]
pub async fn move_folder(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
    destination_id: u32,
) -> Result<FolderMoveResultDto, EditFailure> {
    // Toujours l'historique, puis le catalogue (voir `set_triage`). Un
    // déplacement sur un même disque est instantané : le verrou est tenu
    // pendant le renommage.
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let result = move_folder_in(
        &mut catalog,
        folder_id_of(folder_id),
        folder_id_of(destination_id),
    )
    .map_err(|error| failure(&error))?;
    Ok(match result {
        MoveOutcome::Moved(moved) => {
            let summary = history.record_folder_move(moved);
            FolderMoveResultDto::Moved {
                outcome: outcome(Some(summary), &history),
            }
        }
        MoveOutcome::Refused(refusal) => FolderMoveResultDto::Refused {
            refusal: refusal.into(),
        },
        MoveOutcome::Unknown => FolderMoveResultDto::Unknown,
        MoveOutcome::Failed(reason) => FolderMoveResultDto::Failed { reason },
    })
}

impl From<MoveRefusal> for MoveRefusalDto {
    fn from(refusal: MoveRefusal) -> Self {
        match refusal {
            MoveRefusal::AlreadyThere => Self::AlreadyThere,
            MoveRefusal::IntoItself => Self::IntoItself,
            MoveRefusal::Missing => Self::Missing,
            MoveRefusal::DestinationMissing => Self::DestinationMissing,
            MoveRefusal::OtherDisk => Self::OtherDisk,
            MoveRefusal::CameraCard => Self::CameraCard,
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn folder_id_of(id: u32) -> FolderId {
    FolderId::new(i64::from(id))
}

fn count_dto(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}
