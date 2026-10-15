//! Tâche de fond de fabrication des vignettes, et son déclenchement.
//!
//! Une seule tâche de vignettes tourne à la fois. Si de nouvelles photos
//! arrivent pendant qu'elle tourne (un second import), elle refait un passage
//! à la fin plutôt que d'en lancer une seconde en parallèle.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::domain::MediaId;
use crate::jobs::{JobContext, JobError, PassScheduler};
use crate::replaced_files;
use crate::thumbnails::{GenerationError, PreviewCache, ThumbnailCache, generate_pending};
use crate::workspace::Workspace;

/// Bilan d'une fabrication de vignettes, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailsSummary {
    /// Vignettes fabriquées.
    #[specta(type = u32)]
    pub generated: u64,
    /// Photos ou vidéos que le système ne sait pas décoder.
    #[specta(type = u32)]
    pub failed: u64,
}

/// Déclenchement de la tâche des vignettes (voir [`PassScheduler`]).
#[derive(Debug, Default)]
pub struct ThumbnailScheduler(PassScheduler);

/// Lance la fabrication des vignettes manquantes, sauf si elle tourne déjà
/// (elle refera alors un passage). Sans catalogue ouvert, ne fait rien.
pub fn request_thumbnails(app: &AppHandle) {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return;
    }
    let Some(workspace) = app
        .try_state::<Workspace>()
        .map(|workspace| workspace.inner().clone())
    else {
        return;
    };
    let scheduler = app.state::<ThumbnailScheduler>();
    if !scheduler.0.begin() {
        return;
    }

    let cache = ThumbnailCache::new(workspace.thumbnails_dir());
    let handle = app.clone();
    let started = app
        .state::<AppJobManager>()
        .start(JobKind::Thumbnails, move |context| {
            run_generation(&handle, &cache, context)
        });
    if let Err(error) = started {
        tracing::error!(%error, "cannot start the thumbnail generation");
        scheduler.0.end();
    }
}

/// Travail de la tâche : passages successifs tant que de nouvelles photos
/// sont arrivées entre-temps.
fn run_generation(
    app: &AppHandle,
    cache: &ThumbnailCache,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let scheduler = app.state::<ThumbnailScheduler>();
    let _end = scheduler.0.end_guard();
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let workers = std::thread::available_parallelism()
        .map_or(2, |count| count.get().saturating_sub(1).clamp(1, 6));

    let mut summary = ThumbnailsSummary {
        generated: 0,
        failed: 0,
    };
    loop {
        let report = generate_pending(
            &catalog,
            cache,
            workers,
            context.cancellation(),
            |done, total| {
                context.report_progress_of(done, total);
            },
        )
        .map_err(|error| {
            tracing::warn!(%error, "thumbnail generation stopped");
            match error {
                GenerationError::Cancelled => JobError::Cancelled,
                GenerationError::Catalog(_) | GenerationError::Cache(_) => {
                    JobError::Failed(JobFailure::CatalogError)
                }
            }
        })?;
        summary.generated += report.generated;
        summary.failed += report.failed;
        tracing::info!(?report, "thumbnail pass finished");
        if !scheduler.0.pass_finished() {
            break;
        }
    }
    Ok(JobResult::Thumbnails { summary })
}

/// Prend acte des fichiers remplacés parmi les médias `ids` :
/// leurs aperçus sont oubliés et leurs vignettes refaites. Renvoie le
/// nombre de fichiers remplacés.
pub(super) fn refresh_replaced(app: &AppHandle, ids: &[MediaId]) -> usize {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return 0;
    };
    match replaced_files::refresh(&catalog, ids) {
        Ok(replaced) if !replaced.is_empty() => {
            if let Some(previews) = app.try_state::<PreviewCache>() {
                previews.forget(&replaced);
            }
            request_thumbnails(app);
            replaced.len()
        }
        Ok(_) => 0,
        Err(error) => {
            tracing::warn!(%error, "cannot refresh the replaced files");
            0
        }
    }
}

/// « Synchroniser le dossier » ▸ fichiers modifiés : leurs
/// vignettes et aperçus sont refaits avant la relecture de leurs
/// métadonnées. Renvoie le nombre de fichiers remplacés.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn refresh_replaced_files(app: AppHandle, media_ids: Vec<u32>) -> u32 {
    let ids = super::edits::media_ids_of(media_ids);
    u32::try_from(refresh_replaced(&app, &ids)).unwrap_or(u32::MAX)
}
