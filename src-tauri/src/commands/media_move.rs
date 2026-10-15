//! Déplacer des photos (glisser sur un dossier, nouveau dossier avec la
//! sélection) et renommer un dossier, de façon annulable.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::catalog_info::CatalogReadFailure;
use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use crate::catalog::repositories::folders;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{FolderId, MediaId};
use crate::edits::History;
use crate::files::create_folder;
use crate::moving::{
    self, MediaMoveRefusal, MovePlan, RenameOutcome, RenameRefusal, execute_media_move,
    plan_media_move,
};

/// Raison pour laquelle des photos ne peuvent pas être déplacées, ou un
/// dossier renommé ou créé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FileMoveRefusalDto {
    /// Les photos sont déjà toutes dans ce dossier.
    NothingToMove,
    /// Le fichier d'une photo, ou le dossier, est introuvable.
    Missing,
    /// Le dossier de destination est introuvable.
    DestinationMissing,
    /// La destination est sur un autre disque.
    OtherDisk,
    /// Une carte mémoire est en jeu (rien ne s'y écrit).
    CameraCard,
    /// Nom vide, avec « / » ou « : », ou « . » et « .. ».
    InvalidName,
    /// Un fichier ou un dossier porte déjà ce nom à cet endroit.
    NameTaken,
    /// Le modèle de renommage donne un nom vide.
    EmptyName,
    /// Les noms ne changent pas.
    NothingToRename,
}

impl From<crate::renaming::RenameRefusal> for FileMoveRefusalDto {
    fn from(refusal: crate::renaming::RenameRefusal) -> Self {
        use crate::renaming::RenameRefusal;
        match refusal {
            RenameRefusal::EmptyName => Self::EmptyName,
            RenameRefusal::NothingToRename => Self::NothingToRename,
            RenameRefusal::Missing => Self::Missing,
            RenameRefusal::CameraCard => Self::CameraCard,
        }
    }
}

impl From<MediaMoveRefusal> for FileMoveRefusalDto {
    fn from(refusal: MediaMoveRefusal) -> Self {
        match refusal {
            MediaMoveRefusal::NothingToMove => Self::NothingToMove,
            MediaMoveRefusal::Missing => Self::Missing,
            MediaMoveRefusal::DestinationMissing => Self::DestinationMissing,
            MediaMoveRefusal::OtherDisk => Self::OtherDisk,
            MediaMoveRefusal::CameraCard => Self::CameraCard,
        }
    }
}

impl From<RenameRefusal> for FileMoveRefusalDto {
    fn from(refusal: RenameRefusal) -> Self {
        match refusal {
            RenameRefusal::InvalidName => Self::InvalidName,
            RenameRefusal::NameTaken => Self::NameTaken,
            RenameRefusal::Missing => Self::Missing,
            RenameRefusal::CameraCard => Self::CameraCard,
        }
    }
}

/// Ce qu'un déplacement de photos ferait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaMovePreviewDto {
    /// Photos et vidéos déplacées (l'autre fichier des paires compris).
    pub media_count: u32,
    /// Fichiers annexes qui les suivent.
    pub sidecar_count: u32,
    /// Nom du dossier de destination.
    pub destination_name: String,
    /// Raison d'un refus, s'il y en a un.
    pub refusal: Option<FileMoveRefusalDto>,
}

/// Résultat d'un déplacement, d'une création ou d'un renommage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileMoveResultDto {
    /// Fait (annulable) ; `null` si rien n'a bougé.
    pub outcome: Option<EditOutcome>,
    /// Raison d'un refus.
    pub refusal: Option<FileMoveRefusalDto>,
    /// Photos (ou paires) laissées en place, refusées par le système.
    pub failed: u32,
}

impl FileMoveResultDto {
    pub(super) fn refused(refusal: impl Into<FileMoveRefusalDto>) -> Self {
        Self {
            outcome: None,
            refusal: Some(refusal.into()),
            failed: 0,
        }
    }
}

fn small(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn folder_path(catalog: &Catalog, id: u32) -> Result<Option<PathBuf>, CatalogError> {
    Ok(folders::list(catalog.connection())?
        .into_iter()
        .find(|folder| folder.id == FolderId::new(i64::from(id)))
        .map(|folder| folder.path))
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Résumé du déplacement des photos dans le dossier `folder_id`.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_media_move(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    folder_id: u32,
) -> Result<MediaMovePreviewDto, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let read = || -> Result<MediaMovePreviewDto, CatalogError> {
        let Some(destination) = folder_path(&catalog, folder_id)? else {
            return Ok(MediaMovePreviewDto {
                media_count: 0,
                sidecar_count: 0,
                destination_name: String::new(),
                refusal: Some(FileMoveRefusalDto::DestinationMissing),
            });
        };
        let plan = plan_media_move(catalog.connection(), &media_ids_of(media_ids), &destination)?;
        Ok(MediaMovePreviewDto {
            media_count: plan.as_ref().map_or(0, |plan| small(plan.media_count())),
            sidecar_count: plan.as_ref().map_or(0, |plan| small(plan.sidecar_count())),
            destination_name: name_of(&destination),
            refusal: plan.err().map(FileMoveRefusalDto::from),
        })
    };
    read().map_err(|error| {
        tracing::error!(%error, "cannot check the photo move");
        CatalogReadFailure::CatalogUnavailable
    })
}

/// Exécute un plan et l'ajoute à l'historique.
pub(super) fn run(
    history: &mut History,
    catalog: &mut Catalog,
    plan: MovePlan,
) -> Result<FileMoveResultDto, CatalogError> {
    let result = execute_media_move(catalog, plan)?;
    let failed = small(result.failed);
    Ok(FileMoveResultDto {
        outcome: result.done.map(|done| {
            let summary = history.record_media_move(done);
            outcome(Some(summary), history)
        }),
        refusal: None,
        failed,
    })
}

/// Déplace les photos (avec l'autre fichier de leur paire et leurs fichiers
/// annexes) dans le dossier `folder_id`, sur le même disque ; Cmd+Z les
/// remet à leur place.
///
/// # Errors
///
/// Voir [`EditFailure`].
#[tauri::command]
#[specta::specta]
pub async fn move_media(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    folder_id: u32,
) -> Result<FileMoveResultDto, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let act = || -> Result<FileMoveResultDto, CatalogError> {
        let Some(destination) = folder_path(&catalog, folder_id)? else {
            return Ok(FileMoveResultDto::refused(
                MediaMoveRefusal::DestinationMissing,
            ));
        };
        match plan_media_move(catalog.connection(), &media_ids_of(media_ids), &destination)? {
            Ok(plan) => run(&mut history, &mut catalog, plan),
            Err(refusal) => Ok(FileMoveResultDto::refused(refusal)),
        }
    };
    act().map_err(|error| failure(&error))
}

/// Crée le dossier `name` dans le dossier `parent_id` et y déplace les
/// photos (le catalogue ne garde jamais un dossier vide, d'où un dossier créé avec des photos).
///
/// # Errors
///
/// Voir [`EditFailure`].
#[tauri::command]
#[specta::specta]
pub async fn move_media_to_new_folder(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    parent_id: u32,
    name: String,
) -> Result<FileMoveResultDto, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let name = name.trim().to_owned();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Ok(FileMoveResultDto::refused(RenameRefusal::InvalidName));
    }
    let ids: Vec<MediaId> = media_ids_of(media_ids);
    let mut act = || -> Result<FileMoveResultDto, CatalogError> {
        let Some(parent) = folder_path(&catalog, parent_id)? else {
            return Ok(FileMoveResultDto::refused(
                MediaMoveRefusal::DestinationMissing,
            ));
        };
        // Vérifie d'abord les photos (même disque, fichiers présents) : un
        // refus ne laisse aucun dossier créé.
        if let Err(refusal) = plan_media_move(catalog.connection(), &ids, &parent)?
            && refusal != MediaMoveRefusal::NothingToMove
        {
            return Ok(FileMoveResultDto::refused(refusal));
        }
        let created = match create_folder(&parent, &name) {
            Ok(created) => created,
            Err(error) => {
                tracing::warn!(%error, "cannot create the folder");
                return Ok(FileMoveResultDto::refused(RenameRefusal::NameTaken));
            }
        };
        match plan_media_move(catalog.connection(), &ids, &created)? {
            Ok(plan) => run(&mut history, &mut catalog, plan),
            Err(refusal) => Ok(FileMoveResultDto::refused(refusal)),
        }
    };
    act().map_err(|error| failure(&error))
}

/// Renomme le dossier `folder_id` (Cmd+Z lui rend son nom).
///
/// # Errors
///
/// Voir [`EditFailure`].
#[tauri::command]
#[specta::specta]
pub async fn rename_folder(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
    name: String,
) -> Result<FileMoveResultDto, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let result = moving::rename_folder(&mut catalog, FolderId::new(i64::from(folder_id)), &name)
        .map_err(|error| failure(&error))?;
    Ok(match result {
        RenameOutcome::Renamed(renamed) => {
            let summary = history.record_folder_move(renamed);
            FileMoveResultDto {
                outcome: Some(outcome(Some(summary), &history)),
                refusal: None,
                failed: 0,
            }
        }
        RenameOutcome::Refused(refusal) => FileMoveResultDto::refused(refusal),
        RenameOutcome::Unknown => FileMoveResultDto::refused(RenameRefusal::Missing),
        RenameOutcome::Failed(reason) => {
            tracing::warn!(%reason, "cannot rename the folder");
            FileMoveResultDto {
                outcome: None,
                refusal: None,
                failed: 1,
            }
        }
    })
}
