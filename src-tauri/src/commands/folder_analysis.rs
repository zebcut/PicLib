//! Analyse d'une source avant import(écran d'import) :
//! parcours en tâche de fond, date de prise de vue et état de chaque
//! fichier, gardés dans le cœur le temps que l'écran d'import les montre.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use jiff::tz::TimeZone;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use super::jobs::{JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::domain::MediaFormat;
use crate::files::{ScanError, ScanReport, SourceScan, scan_source};
use crate::import::{SourceFile, read_source};
use crate::jobs::{JobContext, JobError};
use crate::platform;
use crate::workspace::Workspace;

/// Source analysée : son dossier, puis ses fichiers une fois lus.
#[derive(Debug, Clone)]
pub struct AnalyzedSource {
    /// Dossier analysé.
    pub root: PathBuf,
    /// Parcours : sous-dossiers compris ou non, dossiers exclus ; l'import
    /// reprend le même.
    pub scan: SourceScan,
    /// Photos et vidéos, dans l'ordre de l'écran d'import ; `None` tant que
    /// l'analyse n'est pas terminée.
    pub files: Option<Vec<SourceFile>>,
}

/// Sources analysées, par identifiant de tâche d'analyse : l'import et les
/// vignettes de l'écran d'import retrouvent ainsi les fichiers sans que
/// l'interface ait à renvoyer un chemin.
#[derive(Debug, Default)]
pub struct AnalyzedFolders(Mutex<HashMap<u32, AnalyzedSource>>);

impl AnalyzedFolders {
    fn set_files(&self, job_id: u32, files: Vec<SourceFile>) {
        if let Some(source) = self.lock().get_mut(&job_id) {
            source.files = Some(files);
        }
    }

    /// Source d'une analyse.
    #[must_use]
    pub fn get(&self, job_id: u32) -> Option<AnalyzedSource> {
        self.lock().get(&job_id).cloned()
    }

    /// Fichier `index` d'une analyse terminée : son chemin et son format.
    #[must_use]
    pub fn file(&self, job_id: u32, index: usize) -> Option<(PathBuf, MediaFormat)> {
        self.lock()
            .get(&job_id)?
            .files
            .as_ref()?
            .get(index)
            .map(|source| (source.file.path.clone(), source.file.format))
    }

    /// Oublie une analyse.
    pub fn forget(&self, job_id: u32) -> Option<AnalyzedSource> {
        self.lock().remove(&job_id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, AnalyzedSource>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Résumé d'une analyse de dossier.
///
/// Les compteurs sont déclarés `u32` pour TypeScript (`number`, jamais
/// `null`), mais envoyés avec leur valeur exacte : un nombre JavaScript reste
/// exact jusqu'à 2⁵³ (9 millions de milliards), largement assez pour des
/// fichiers et des octets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderAnalysisSummary {
    /// Photos, RAW compris.
    #[specta(type = u32)]
    pub photo_count: u64,
    /// Photos RAW (incluses dans `photo_count`).
    #[specta(type = u32)]
    pub raw_count: u64,
    /// Vidéos.
    #[specta(type = u32)]
    pub video_count: u64,
    /// Dossiers parcourus.
    #[specta(type = u32)]
    pub folder_count: u64,
    /// Autres fichiers, ignorés.
    #[specta(type = u32)]
    pub ignored_count: u64,
    /// Taille totale des photos et vidéos, en octets.
    #[specta(type = u32)]
    pub total_size_bytes: u64,
    /// Nombre d'éléments illisibles.
    #[specta(type = u32)]
    pub unreadable_count: u64,
    /// Quelques éléments illisibles, pour aider l'utilisateur à les trouver.
    pub unreadable_examples: Vec<String>,
    /// Le dossier est sur une carte mémoire ou un appareil photo : il ne
    /// pourra pas être importé par référence.
    pub is_camera_card: bool,
}

/// Nombre maximal d'éléments illisibles détaillés dans un résumé.
pub const UNREADABLE_EXAMPLES: usize = 5;

impl FolderAnalysisSummary {
    fn new(report: &ScanReport, is_camera_card: bool) -> Self {
        let summary = report.summary();
        Self {
            photo_count: summary.photo_count,
            raw_count: summary.raw_count,
            video_count: summary.video_count,
            folder_count: report.folder_count,
            ignored_count: report.ignored_count,
            total_size_bytes: summary.total_size,
            unreadable_count: report.issues.len() as u64,
            unreadable_examples: report
                .issues
                .iter()
                .take(UNREADABLE_EXAMPLES)
                .map(|issue| issue.path.display().to_string())
                .collect(),
            is_camera_card,
        }
    }
}

/// Raison pour laquelle l'analyse n'a pas démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnalyzeFolderFailure {
    /// La fenêtre de sélection a renvoyé un emplacement inutilisable.
    UnsupportedLocation,
    /// Cette source est inconnue.
    UnknownSource,
    /// La tâche de fond n'a pas pu être lancée.
    JobNotStarted,
}

/// Lance l'analyse de `folder` en tâche de fond ; le résultat arrive par
/// l'abonnement aux tâches, les fichiers par [`AnalyzedFolders`].
///
/// # Errors
///
/// [`AnalyzeFolderFailure::JobNotStarted`] si la tâche n'a pas pu être
/// lancée.
pub fn start_analysis(
    app: &AppHandle,
    folder: PathBuf,
    include_subfolders: bool,
) -> Result<u32, AnalyzeFolderFailure> {
    let jobs = app
        .try_state::<super::jobs::AppJobManager>()
        .ok_or(AnalyzeFolderFailure::JobNotStarted)?;
    let analyzed = app
        .try_state::<AnalyzedFolders>()
        .ok_or(AnalyzeFolderFailure::JobNotStarted)?;
    let kind = JobKind::FolderAnalysis {
        folder: folder.display().to_string(),
    };
    // La source est notée avant que la tâche puisse y ranger ses fichiers.
    let mut analyzed_folders = analyzed.lock();
    let scan = SourceScan {
        include_subfolders,
        // Les caches et sauvegardes du catalogue ne sont jamais proposés.
        excluded: app
            .try_state::<Workspace>()
            .map(|workspace| workspace.internal_dirs())
            .unwrap_or_default(),
    };
    let task_app = app.clone();
    let analyzed_folder = folder.clone();
    let task_scan = scan.clone();
    let job_id = jobs
        .start(kind, move |context| {
            analyze(&task_app, &analyzed_folder, &task_scan, context)
        })
        .map_err(|error| {
            tracing::error!(%error, "cannot start the folder analysis");
            AnalyzeFolderFailure::JobNotStarted
        })?;
    analyzed_folders.insert(
        job_id.get(),
        AnalyzedSource {
            root: folder,
            scan,
            files: None,
        },
    );
    drop(analyzed_folders);
    Ok(job_id.get())
}

/// Travail de la tâche : parcourt le dossier, lit la date de chaque
/// fichier et le compare au catalogue, puis résume ce qu'il contient.
fn analyze(
    app: &AppHandle,
    folder: &Path,
    scan: &SourceScan,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let report = scan_source(folder, scan, context.cancellation(), |found| {
        context.report_progress(found);
    })
    .map_err(|error| {
        tracing::warn!(%error, "folder analysis stopped");
        scan_failure(error)
    })?;

    for issue in &report.issues {
        tracing::warn!(path = %issue.path.display(), reason = %issue.reason, "unreadable item during analysis");
    }
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let files = read_source(
        &catalog,
        &report,
        &TimeZone::system(),
        context.cancellation(),
        |done, total| context.report_progress_of(done, total),
    )
    .map_err(|error| {
        tracing::error!(%error, "cannot compare the source with the catalog");
        JobError::Failed(JobFailure::CatalogError)
    })?
    .ok_or(JobError::Cancelled)?;
    if let Some(analyzed) = app.try_state::<AnalyzedFolders>() {
        analyzed.set_files(context.id().get(), files);
    }
    let is_camera_card = platform::is_on_camera_card(folder);
    Ok(JobResult::FolderAnalysis {
        summary: FolderAnalysisSummary::new(&report, is_camera_card),
    })
}

/// Traduit une erreur de parcours en fin de tâche.
pub fn scan_failure(error: ScanError) -> JobError<JobFailure> {
    match error {
        ScanError::Cancelled => JobError::Cancelled,
        ScanError::SourceUnavailable { path, .. } => {
            JobError::Failed(JobFailure::SourceUnavailable {
                path: path.display().to_string(),
            })
        }
        ScanError::NotAFolder { path } => JobError::Failed(JobFailure::NotAFolder {
            path: path.display().to_string(),
        }),
    }
}
