//! File ▸ "Clean Up and Optimize Catalog…": a check in the
//! background, a summary to confirm in the interface, then the cleanup in
//! the background (see [`crate::maintenance`]).

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::jobs::{JobContext, JobError};
use crate::maintenance::{self, Caches, CheckReport, CleanupReport, MaintenanceError};
use crate::thumbnails::{PreviewCache, ThumbnailCache};
use crate::workspace::Workspace;

/// What a cleanup would do, for the confirmation dialog (counters typed
/// `u32` for TypeScript, sent with their exact value; sizes in bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCheckSummary {
    /// Integrity problems (at most ten); empty when the catalog is sound.
    pub problems: Vec<String>,
    /// Thumbnails of media no longer in the catalog.
    #[specta(type = u32)]
    pub orphan_thumbnails: u64,
    /// Previews of media no longer in the catalog.
    #[specta(type = u32)]
    pub orphan_previews: u64,
    /// Their size.
    #[specta(type = u32)]
    pub orphan_bytes: u64,
    /// Stacks left with fewer than two photos.
    #[specta(type = u32)]
    pub lone_stacks: u64,
    /// Size of the catalog.
    #[specta(type = u32)]
    pub catalog_bytes: u64,
    /// Unused space in the catalog.
    #[specta(type = u32)]
    pub reclaimable_bytes: u64,
}

impl From<CheckReport> for CatalogCheckSummary {
    fn from(report: CheckReport) -> Self {
        Self {
            problems: report.problems,
            orphan_thumbnails: report.orphan_thumbnails,
            orphan_previews: report.orphan_previews,
            orphan_bytes: report.orphan_bytes,
            lone_stacks: report.lone_stacks,
            catalog_bytes: report.catalog_bytes,
            reclaimable_bytes: report.reclaimable_bytes,
        }
    }
}

/// What a cleanup did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCleanupSummary {
    /// File name of the backup made first (in the backups folder).
    pub backup_name: String,
    /// Thumbnails and previews removed.
    #[specta(type = u32)]
    pub removed_images: u64,
    /// Their size.
    #[specta(type = u32)]
    pub removed_bytes: u64,
    /// Stacks removed.
    #[specta(type = u32)]
    pub removed_stacks: u64,
    /// Size of the catalog before.
    #[specta(type = u32)]
    pub catalog_bytes_before: u64,
    /// Size of the catalog after.
    #[specta(type = u32)]
    pub catalog_bytes_after: u64,
}

impl From<CleanupReport> for CatalogCleanupSummary {
    fn from(report: CleanupReport) -> Self {
        Self {
            backup_name: report
                .backup
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            removed_images: report.removed_images,
            removed_bytes: report.removed_bytes,
            removed_stacks: report.removed_stacks,
            catalog_bytes_before: report.catalog_bytes_before,
            catalog_bytes_after: report.catalog_bytes_after,
        }
    }
}

/// Starts the check of the catalog; its summary arrives through the jobs
/// subscription. Nothing is changed.
// Tauri ne sait passer `AppHandle` à une commande que par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn check_catalog(app: AppHandle) {
    start(&app, JobKind::CatalogCheck, run_check);
}

/// Starts the cleanup of the catalog, once the user confirmed the summary
/// of a check.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn clean_catalog(app: AppHandle) {
    start(&app, JobKind::CatalogCleanup, run_cleanup);
}

type Work = fn(
    &AppHandle,
    &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>>;

fn start(app: &AppHandle, kind: JobKind, work: Work) {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return;
    }
    let handle = app.clone();
    let started = app
        .state::<AppJobManager>()
        .start(kind, move |context| work(&handle, context));
    if let Err(error) = started {
        tracing::error!(%error, "cannot start the catalog maintenance");
    }
}

fn run_check(
    app: &AppHandle,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let (catalog, workspace, previews) = states(app)?;
    let thumbnails = ThumbnailCache::new(workspace.thumbnails_dir());
    let report = maintenance::check(
        &catalog,
        Caches {
            thumbnails: &thumbnails,
            previews: &previews,
        },
        context.cancellation(),
        |done, total| context.report_progress_of(done, total),
    )
    .map_err(job_error)?;
    tracing::info!(?report, "catalog checked");
    Ok(JobResult::CatalogCheck {
        summary: report.into(),
    })
}

fn run_cleanup(
    app: &AppHandle,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let (catalog, workspace, previews) = states(app)?;
    let thumbnails = ThumbnailCache::new(workspace.thumbnails_dir());
    let report = maintenance::clean(
        &catalog,
        Caches {
            thumbnails: &thumbnails,
            previews: &previews,
        },
        &workspace.backups_dir(),
        context.cancellation(),
        |done, total| context.report_progress_of(done, total),
    )
    .map_err(job_error)?;
    tracing::info!(?report, "catalog cleaned up");
    Ok(JobResult::CatalogCleanup {
        summary: report.into(),
    })
}

type States<'a> = (
    tauri::State<'a, Mutex<Catalog>>,
    tauri::State<'a, Workspace>,
    tauri::State<'a, PreviewCache>,
);

fn states(app: &AppHandle) -> Result<States<'_>, JobError<JobFailure>> {
    match (
        app.try_state::<Mutex<Catalog>>(),
        app.try_state::<Workspace>(),
        app.try_state::<PreviewCache>(),
    ) {
        (Some(catalog), Some(workspace), Some(previews)) => Ok((catalog, workspace, previews)),
        _ => Err(JobError::Failed(JobFailure::CatalogError)),
    }
}

fn job_error(error: MaintenanceError) -> JobError<JobFailure> {
    match error {
        MaintenanceError::Cancelled => JobError::Cancelled,
        MaintenanceError::Damaged => {
            tracing::error!("the catalog failed its integrity check, not cleaned up");
            JobError::Failed(JobFailure::CatalogDamaged)
        }
        MaintenanceError::Backup(error) => {
            tracing::error!(%error, "no backup before the cleanup, nothing changed");
            JobError::Failed(JobFailure::BackupFailed)
        }
        MaintenanceError::Catalog(error) => {
            tracing::error!(%error, "catalog maintenance stopped");
            JobError::Failed(JobFailure::CatalogError)
        }
    }
}
