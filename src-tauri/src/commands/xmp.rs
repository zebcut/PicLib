//! Fichiers XMP : « Enregistrer les métadonnées dans les
//! fichiers », en tâche de fond, et écriture automatique
//! après les modifications quand la préférence est cochée.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::catalog_info::CatalogReadFailure;
use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use super::preferences::AppPreferences;
use crate::catalog::repositories::xmp_sync;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::MediaId;
use crate::edits::{Edit, History, SidecarRead};
use crate::files;
use crate::jobs::{JobContext, JobError, PassScheduler};
use crate::metadata::xmp;
use crate::workspace::Workspace;
use crate::xmp_files::{self, CheckReport, WriteError, WriteReport, WriteSettings};

/// Bilan d'une écriture des XMP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct XmpWriteSummary {
    /// Fichiers écrits.
    #[specta(type = u32)]
    pub written: u64,
    /// Fichiers modifiés par un autre logiciel, laissés tels quels.
    #[specta(type = u32)]
    pub changed_elsewhere: u64,
    /// Échecs (photo introuvable, carte mémoire, fichier illisible...).
    #[specta(type = u32)]
    pub failed: u64,
}

impl From<WriteReport> for XmpWriteSummary {
    fn from(report: WriteReport) -> Self {
        Self {
            written: report.written,
            changed_elsewhere: report.changed_elsewhere,
            failed: report.failed,
        }
    }
}

/// Raison pour laquelle l'écriture n'a pas démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum XmpWriteStartFailure {
    /// Aucun catalogue n'est ouvert.
    CatalogUnavailable,
    /// La tâche de fond n'a pas pu être lancée.
    JobNotStarted,
}

/// Écrit, en tâche de fond, les métadonnées des médias `media_ids` dans
/// leurs fichiers XMP. `overwrite_changed` : remplacer aussi les fichiers
/// modifiés par un autre logiciel (« Écraser avec le catalogue »).
///
/// # Errors
///
/// Voir [`XmpWriteStartFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn save_metadata_to_files(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    media_ids: Vec<u32>,
    overwrite_changed: bool,
) -> Result<(), XmpWriteStartFailure> {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return Err(XmpWriteStartFailure::CatalogUnavailable);
    }
    let kind = JobKind::XmpWrite {
        count: u32::try_from(media_ids.len()).unwrap_or(u32::MAX),
        automatic: false,
    };
    let ids = media_ids_of(media_ids);
    jobs.start(kind, move |context| {
        run_write(&app, &ids, overwrite_changed, context)
    })
    .map_err(|error| {
        tracing::error!(%error, "cannot start writing the XMP files");
        XmpWriteStartFailure::JobNotStarted
    })?;
    Ok(())
}

fn run_write(
    app: &AppHandle,
    ids: &[MediaId],
    overwrite_changed: bool,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let workspace = app
        .try_state::<Workspace>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let settings = WriteSettings {
        originals_dir: workspace.xmp_originals_dir(),
        metadata_date: now(),
        overwrite_changed,
    };
    let report = xmp_files::write(
        &catalog,
        ids,
        &settings,
        context.cancellation(),
        |done, total| {
            context.report_progress_of(done, total);
        },
    )
    .map_err(|error| match error {
        WriteError::Cancelled => JobError::Cancelled,
        WriteError::Catalog(error) => {
            tracing::warn!(%error, "XMP writing stopped");
            JobError::Failed(JobFailure::CatalogError)
        }
    })?;
    tracing::info!(requested = ids.len(), ?report, "XMP files written");
    Ok(JobResult::XmpWrite {
        summary: report.into(),
    })
}

/// Intervalle entre deux recherches des XMP à écrire, quand l'écriture est
/// automatique : les modifications faites d'affilée s'écrivent ensemble.
const AUTO_WRITE_INTERVAL: Duration = Duration::from_secs(3);

/// Médias écrits au plus par une écriture automatique (les suivants le
/// seront à la suivante).
const AUTO_WRITE_BATCH: u64 = 2000;

/// Une seule écriture automatique à la fois (voir [`PassScheduler`]).
#[derive(Debug, Default)]
pub struct XmpAutoScheduler(PassScheduler);

/// Écrit, quand la préférence est cochée, les XMP des médias « à
/// enregistrer », toutes les quelques secondes, en tâche de fond discrète.
pub fn watch_pending_xmp(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("xmp-writer".to_owned())
        .spawn(move || {
            loop {
                std::thread::sleep(AUTO_WRITE_INTERVAL);
                let wanted = app
                    .try_state::<AppPreferences>()
                    .is_some_and(|preferences| preferences.current().write_xmp_automatically);
                if wanted {
                    request_auto_write(&app);
                }
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot watch for XMP files to write");
    }
}

/// Lance une écriture automatique s'il y a des médias à écrire et qu'aucune
/// ne tourne déjà.
fn request_auto_write(app: &AppHandle) {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return;
    };
    let pending = xmp_sync::writable_pending(
        catalog
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .connection(),
        AUTO_WRITE_BATCH,
    );
    let ids = match pending {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => return,
        Err(error) => {
            tracing::warn!(%error, "cannot look for XMP files to write");
            return;
        }
    };
    let scheduler = app.state::<XmpAutoScheduler>();
    if !scheduler.0.begin() {
        return;
    }
    let kind = JobKind::XmpWrite {
        count: u32::try_from(ids.len()).unwrap_or(u32::MAX),
        automatic: true,
    };
    let handle = app.clone();
    let started = app.state::<AppJobManager>().start(kind, move |context| {
        let scheduler = handle.state::<XmpAutoScheduler>();
        let _end = scheduler.0.end_guard();
        run_write(&handle, &ids, false, context)
    });
    if let Err(error) = started {
        tracing::error!(%error, "cannot start writing the XMP files");
        scheduler.0.end();
    }
}

/// Date et heure locales, au format ISO 8601 avec le décalage UTC.
pub(super) fn now() -> String {
    jiff::Zoned::now()
        .strftime("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Bilan d'une recherche des XMP modifiés ailleurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct XmpCheckDto {
    /// Fichiers modifiés par un autre logiciel depuis que PicLib les a
    /// écrits ou lus.
    #[specta(type = u32)]
    pub changed: u64,
    /// Fichiers disparus (les photos redeviennent « à enregistrer »).
    #[specta(type = u32)]
    pub removed: u64,
}

/// Cherche, dans le dossier `folder_id` et ses sous-dossiers, les XMP qu'un
/// autre logiciel a modifiés ou supprimés : à l'affichage
/// d'un dossier et à sa synchronisation.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn check_folder_sidecars(
    catalog: State<'_, Mutex<Catalog>>,
    folder_id: u32,
) -> Result<XmpCheckDto, CatalogReadFailure> {
    let report = xmp_files::check_folder(&catalog, i64::from(folder_id)).map_err(|error| {
        tracing::warn!(%error, "cannot check the folder's XMP files");
        CatalogReadFailure::CatalogUnavailable
    })?;
    if report != CheckReport::default() {
        tracing::info!(?report, "XMP files changed elsewhere");
    }
    Ok(XmpCheckDto {
        changed: report.changed,
        removed: report.removed,
    })
}

/// « Lire depuis le fichier » : les XMP des médias `media_ids` remplacent ce
/// que le catalogue en sait, de façon annulable ; ils sont ensuite tenus
/// pour à jour. Un média sans XMP lisible est laissé tel quel.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
// Tâche asynchrone : lire les fichiers ne doit pas figer l'interface.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn read_sidecar_metadata(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let sources = xmp_sync::sources(catalog.connection(), &ids).map_err(|error| failure(&error))?;
    let mut files = Vec::new();
    for source in &sources {
        let path = xmp_files::sidecar_path(source);
        let read = files::file_stamp(&path).ok().flatten().and_then(|stamp| {
            let packet = std::fs::read(&path).ok()?;
            Some((stamp, xmp::parse_packet(&packet).ok()?))
        });
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        if let (Some((stamp, read)), Some(name)) = (read, name) {
            files.push((source.id, name, stamp, read));
        } else {
            tracing::warn!(path = %path.display(), "XMP file missing or unreadable");
        }
    }
    let reads: Vec<SidecarRead> = files
        .iter()
        .map(|(id, _, _, read)| SidecarRead {
            media_id: *id,
            metadata: read.metadata.clone(),
        })
        .collect();
    let edit = Edit::sidecar_read(&catalog, &reads).map_err(|error| failure(&error))?;
    let summary = history
        .perform(&mut catalog, edit)
        .map_err(|error| failure(&error))?;
    // Le catalogue est maintenant à jour avec ces fichiers.
    let record = || -> Result<(), CatalogError> {
        for (id, name, stamp, _) in &files {
            xmp_sync::record_read(catalog.connection(), *id, name, *stamp)?;
        }
        Ok(())
    };
    record().map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}
