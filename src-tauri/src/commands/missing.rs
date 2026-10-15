//! Fichiers manquants : la vérification de fond, lancée au
//! démarrage et sur demande (Bibliothèque ▸ « Rechercher les photos
//! manquantes »).

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::jobs::{JobContext, JobError, PassScheduler};
use crate::missing_files::{self, MissingCheckError};

/// Déclenchement de la vérification (voir [`PassScheduler`]).
#[derive(Debug, Default)]
pub struct MissingScheduler(PassScheduler);

/// Bilan d'une vérification, pour l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissingSummary {
    /// Fichiers introuvables.
    #[specta(type = u32)]
    pub missing: u64,
    /// Fichiers manquants retrouvés.
    #[specta(type = u32)]
    pub found_again: u64,
}

/// Lance la vérification, sauf si elle tourne déjà (elle refera alors un
/// passage). Sans catalogue ouvert, ne fait rien.
pub fn request_missing_check(app: &AppHandle) {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return;
    }
    let scheduler = app.state::<MissingScheduler>();
    if !scheduler.0.begin() {
        return;
    }
    let handle = app.clone();
    let started = app
        .state::<AppJobManager>()
        .start(JobKind::MissingFiles, move |context| {
            run_check(&handle, context)
        });
    if let Err(error) = started {
        tracing::error!(%error, "cannot start the missing files check");
        scheduler.0.end();
    }
}

/// Cherche de nouveau les fichiers de tout le catalogue.
// Tauri ne sait passer `AppHandle` à une commande que par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn check_missing_files(app: AppHandle) {
    request_missing_check(&app);
}

fn run_check(
    app: &AppHandle,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let scheduler = app.state::<MissingScheduler>();
    let _end = scheduler.0.end_guard();
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let mut summary = MissingSummary {
        missing: 0,
        found_again: 0,
    };
    loop {
        let report = missing_files::check(&catalog, context.cancellation(), |done, total| {
            context.report_progress_of(done, total);
        })
        .map_err(|error| match error {
            MissingCheckError::Cancelled => JobError::Cancelled,
            MissingCheckError::Catalog(error) => {
                tracing::warn!(%error, "missing files check stopped");
                JobError::Failed(JobFailure::CatalogError)
            }
        })?;
        tracing::info!(?report, "missing files check finished");
        summary.missing = report.missing;
        summary.found_again += report.found_again;
        if !scheduler.0.pass_finished() {
            break;
        }
    }
    Ok(JobResult::MissingFiles { summary })
}
