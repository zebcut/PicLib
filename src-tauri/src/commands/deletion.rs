//! Supprimer des photos : la sélection (Suppr) ou les photos
//! rejetées d'une source (Cmd+Retour arrière) ; liste, retrait du catalogue
//! (annulable), mise à la corbeille avec leurs fichiers annexes (tâche de
//! fond).

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::catalog_info::{CatalogReadFailure, GridFilter, GridSource, resolve_query};
use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::repositories::library::{self, RejectedMedia};
use crate::catalog::repositories::media;
use crate::catalog::{Catalog, CatalogError};
use crate::deletion::{TrashItem, TrashMediaError, TrashReport, trash_media};
use crate::domain::MediaId;
use crate::edits::{Edit, History};
use crate::files::SystemTrash;
use crate::jobs::{JobContext, JobError};
use crate::media_files;
use crate::stacks;
use crate::thumbnails::{PreviewCache, ThumbnailCache};
use crate::workspace::Workspace;

/// Nombre de noms de fichiers montrés dans la fenêtre de confirmation.
const LISTED_NAMES: usize = 200;

/// Photos rejetées d'une source, pour la fenêtre de confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RejectedSummary {
    /// Nombre de photos et vidéos rejetées.
    pub count: u32,
    /// Noms des premiers fichiers (au plus 200).
    pub file_names: Vec<String>,
}

/// Bilan d'une mise à la corbeille, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashSummary {
    /// Fichiers placés dans la corbeille.
    #[specta(type = u32)]
    pub trashed: u64,
    /// Fichiers introuvables, laissés au catalogue.
    #[specta(type = u32)]
    pub missing: u64,
    /// Fichiers refusés par le système, laissés à leur place.
    #[specta(type = u32)]
    pub failed: u64,
}

/// Mise à la corbeille lancée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashStarted {
    /// Tâche de fond ; `null` s'il n'y avait aucune photo rejetée.
    pub job_id: Option<u32>,
}

/// Raison pour laquelle la mise à la corbeille n'a pas démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrashStartFailure {
    /// Le catalogue n'est pas ouvert ou est illisible.
    CatalogUnavailable,
    /// Le système n'a pas pu lancer la tâche de fond.
    JobNotStarted,
}

/// Photos rejetées d'une source.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_rejected(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
) -> Result<RejectedSummary, CatalogReadFailure> {
    let rejected = rejected_of(&catalog, source, &filter)
        .map_err(|_error| CatalogReadFailure::CatalogUnavailable)?;
    Ok(RejectedSummary {
        count: u32::try_from(rejected.len()).unwrap_or(u32::MAX),
        file_names: rejected
            .into_iter()
            .take(LISTED_NAMES)
            .map(|media| media.file_name)
            .collect(),
    })
}

/// Retire du catalogue les photos rejetées d'une source, de façon annulable ;
/// les fichiers ne sont pas touchés.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn remove_rejected_from_catalog(
    app: AppHandle,
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
) -> Result<EditOutcome, EditFailure> {
    let ids: Vec<MediaId> = rejected_of(&catalog, source, &filter)
        .map_err(|error| failure(&error))?
        .into_iter()
        .map(|media| media.id)
        .collect();
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = Edit::removal(&catalog, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    // Un aperçu en cache se refabrique au besoin ; la vignette est gardée
    // pour une éventuelle annulation.
    if let Some(previews) = app.try_state::<PreviewCache>() {
        previews.forget(&ids);
    }
    Ok(outcome(Some(summary), &history))
}

/// Place dans la corbeille les fichiers des photos rejetées d'une source,
/// en tâche de fond, puis les retire du catalogue. Pas d'annulation par
/// Cmd+Z : les fichiers se récupèrent depuis la corbeille du système.
///
/// # Errors
///
/// Voir [`TrashStartFailure`].
#[tauri::command]
#[specta::specta]
pub async fn trash_rejected(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
) -> Result<TrashStarted, TrashStartFailure> {
    let ids: Vec<MediaId> = rejected_of(&catalog, source, &filter)
        .map_err(|_error| TrashStartFailure::CatalogUnavailable)?
        .into_iter()
        .map(|media| media.id)
        .collect();
    start_trash(app, &jobs, &catalog, &ids)
}

/// Lance la mise à la corbeille des fichiers des médias `ids` et de leurs
/// fichiers annexes, en tâche de fond.
pub(super) fn start_trash(
    app: AppHandle,
    jobs: &AppJobManager,
    catalog: &Mutex<Catalog>,
    ids: &[MediaId],
) -> Result<TrashStarted, TrashStartFailure> {
    let items: Vec<TrashItem> = {
        let catalog = lock(catalog);
        media_files::files_of(catalog.connection(), ids)
            .map_err(|error| {
                tracing::error!(%error, "cannot read the files to trash");
                TrashStartFailure::CatalogUnavailable
            })?
            .into_iter()
            .map(TrashItem::from)
            .collect()
    };
    if items.is_empty() {
        return Ok(TrashStarted { job_id: None });
    }
    let kind = JobKind::Trash {
        count: u32::try_from(items.len()).unwrap_or(u32::MAX),
    };
    let job_id = jobs
        .start(kind, move |context| run_trash(&app, &items, context))
        .map_err(|error| {
            tracing::error!(%error, "cannot start moving files to the trash");
            TrashStartFailure::JobNotStarted
        })?;
    Ok(TrashStarted {
        job_id: Some(job_id.get()),
    })
}

fn run_trash(
    app: &AppHandle,
    items: &[TrashItem],
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let thumbnails = app
        .try_state::<Workspace>()
        .map(|workspace| ThumbnailCache::new(workspace.thumbnails_dir()));
    let previews = app.try_state::<PreviewCache>();
    let total = items.len() as u64;
    let report = trash_media(
        &catalog,
        &SystemTrash,
        items,
        context.cancellation(),
        |done| context.report_progress_of(done, total),
        |removed| {
            // Ces médias ne reviendront pas : leurs copies réduites sont
            // effacées.
            for &media_id in removed {
                if let Some(thumbnails) = &thumbnails {
                    thumbnails.remove(media_id);
                }
            }
            if let Some(previews) = &previews {
                previews.forget(removed);
            }
        },
    )
    .map_err(|error| match error {
        TrashMediaError::Cancelled => JobError::Cancelled,
        TrashMediaError::Catalog(error) => {
            tracing::error!(%error, "moving files to the trash stopped");
            JobError::Failed(JobFailure::CatalogError)
        }
    })?;
    tracing::info!(?report, "photos moved to the trash");
    Ok(JobResult::Trash {
        summary: TrashSummary::from(report),
    })
}

/// Photos à supprimer (Suppr) : leur nombre, les premiers noms et le nombre
/// de fichiers annexes, pour la fenêtre de confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDeletionSummary {
    /// Photos et vidéos (toute pile repliée sélectionnée comprise).
    pub count: u32,
    /// Noms des premiers fichiers (au plus 200).
    pub file_names: Vec<String>,
    /// Photos des piles repliées ajoutées à la sélection.
    pub from_stacks: u32,
}

/// Médias de la sélection, avec toutes les photos des piles repliées
/// sélectionnées (elles partent en entier).
fn deletion_ids(catalog: &Catalog, media_ids: Vec<u32>) -> Result<Vec<MediaId>, CatalogError> {
    stacks::with_collapsed_members(catalog, &media_ids_of(media_ids))
}

/// Ce que Suppr emporterait.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_photo_deletion(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<PhotoDeletionSummary, CatalogReadFailure> {
    let catalog = lock(&catalog);
    let selected = media_ids.len();
    let read = || -> Result<_, CatalogError> {
        let ids = deletion_ids(&catalog, media_ids)?;
        let found = media::read_with_paths(catalog.connection(), &ids)?;
        Ok(PhotoDeletionSummary {
            count: u32::try_from(found.len()).unwrap_or(u32::MAX),
            from_stacks: u32::try_from(ids.len().saturating_sub(selected)).unwrap_or(u32::MAX),
            file_names: found
                .into_iter()
                .take(LISTED_NAMES)
                .map(|(media, _)| media.data.file_name)
                .collect(),
        })
    };
    read().map_err(|error| {
        tracing::error!(%error, "cannot read the photos to delete");
        CatalogReadFailure::CatalogUnavailable
    })
}

/// Retire du catalogue les photos sélectionnées (et les piles repliées
/// sélectionnées), de façon annulable ; les fichiers ne sont pas touchés.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn remove_photos_from_catalog(
    app: AppHandle,
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<EditOutcome, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let ids = deletion_ids(&catalog, media_ids).map_err(|error| failure(&error))?;
    let summary = Edit::removal(&catalog, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    if let Some(previews) = app.try_state::<PreviewCache>() {
        previews.forget(&ids);
    }
    Ok(outcome(Some(summary), &history))
}

/// Place dans la corbeille les fichiers des photos sélectionnées (et des
/// piles repliées sélectionnées), avec leurs fichiers annexes, en tâche de
/// fond, puis les retire du catalogue. Pas d'annulation par Cmd+Z.
///
/// # Errors
///
/// Voir [`TrashStartFailure`].
#[tauri::command]
#[specta::specta]
pub async fn trash_photos(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<TrashStarted, TrashStartFailure> {
    let ids = {
        let catalog = lock(&catalog);
        deletion_ids(&catalog, media_ids).map_err(|error| {
            tracing::error!(%error, "cannot read the photos to trash");
            TrashStartFailure::CatalogUnavailable
        })?
    };
    start_trash(app, &jobs, &catalog, &ids)
}

/// Photos rejetées d'une source, filtrées ; une erreur est journalisée ici.
fn rejected_of(
    catalog: &Mutex<Catalog>,
    source: GridSource,
    filter: &GridFilter,
) -> Result<Vec<RejectedMedia>, CatalogError> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    let connection = catalog.connection();
    resolve_query(connection, source, filter, None)
        .and_then(|resolved| {
            resolved.map_or(Ok(Vec::new()), |resolved| {
                library::rejected(connection, resolved)
            })
        })
        .inspect_err(|error| tracing::error!(%error, "cannot read the rejected photos"))
}

impl From<TrashReport> for TrashSummary {
    fn from(report: TrashReport) -> Self {
        Self {
            trashed: report.trashed,
            missing: report.missing,
            failed: report.failed,
        }
    }
}
