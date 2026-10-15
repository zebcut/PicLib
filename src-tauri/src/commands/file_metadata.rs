//! « Lire les métadonnées des fichiers » : lecture des fichiers
//! en tâche de fond et aperçu de ce qui serait complété, puis, après
//! confirmation, application annulable (Cmd+Z).

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::MediaId;
use crate::edits::{Edit, FileMetadataCounts, FileMetadataEdit, History};
use crate::jobs::{JobContext, JobError};
use crate::rereading::{FoundMetadata, ReadError, ReadReport, read_files};

/// Dernière lecture qui attend d'être appliquée : une nouvelle lecture la
/// remplace.
#[derive(Debug, Default)]
pub struct PendingFileMetadata(Mutex<Option<PendingRead>>);

#[derive(Debug)]
struct PendingRead {
    job_id: u32,
    found: Vec<FoundMetadata>,
}

impl PendingFileMetadata {
    fn keep(&self, job_id: u32, found: Vec<FoundMetadata>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(PendingRead { job_id, found });
    }

    /// Retire et renvoie ce qu'a trouvé la lecture `job_id`, si c'est bien
    /// la dernière.
    fn take(&self, job_id: u32) -> Option<Vec<FoundMetadata>> {
        let mut pending = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.as_ref().is_some_and(|read| read.job_id == job_id) {
            pending.take().map(|read| read.found)
        } else {
            None
        }
    }
}

/// Bilan d'une lecture, pour la fenêtre de confirmation (compteurs typés
/// `u32` pour TypeScript, envoyés avec leur valeur exacte).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadataSummary {
    /// Médias dont les fichiers ont été demandés.
    #[specta(type = u32)]
    pub requested: u64,
    /// Fichiers introuvables ou illisibles.
    #[specta(type = u32)]
    pub missing: u64,
    /// Fichiers dont une source de métadonnées n'a pas pu être lue.
    #[specta(type = u32)]
    pub unreadable_metadata: u64,
    /// Médias qui seraient complétés.
    #[specta(type = u32)]
    pub to_complete: u64,
    /// … dont la note, le marqueur ou la couleur.
    #[specta(type = u32)]
    pub triage: u64,
    /// … dont des champs descriptifs.
    #[specta(type = u32)]
    pub fields: u64,
    /// … dont des mots-clés.
    #[specta(type = u32)]
    pub keywords: u64,
    /// … dont la position GPS.
    #[specta(type = u32)]
    pub gps: u64,
    /// … dont la date de prise de vue.
    #[specta(type = u32)]
    pub capture_time: u64,
    /// … dont des informations techniques.
    #[specta(type = u32)]
    pub details: u64,
}

/// Lecture lancée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadataStarted {
    /// Identifiant de la tâche, pour suivre sa progression.
    pub job_id: u32,
}

/// Raison pour laquelle la lecture n'a pas démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileMetadataStartFailure {
    /// Aucun catalogue n'est ouvert.
    CatalogUnavailable,
    /// La tâche de fond n'a pas pu être lancée.
    JobNotStarted,
}

/// Lit, en tâche de fond, les métadonnées des fichiers des médias indiqués
/// (fichiers annexes, XMP et IPTC intégrés, EXIF), sans rien modifier. Le
/// bilan dit ce qui serait complété ; [`apply_file_metadata`] l'applique.
///
/// # Errors
///
/// Voir [`FileMetadataStartFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn read_file_metadata(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    media_ids: Vec<u32>,
) -> Result<FileMetadataStarted, FileMetadataStartFailure> {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return Err(FileMetadataStartFailure::CatalogUnavailable);
    }
    let kind = JobKind::FileMetadata {
        count: u32::try_from(media_ids.len()).unwrap_or(u32::MAX),
    };
    let ids = media_ids_of(media_ids);
    let job_id = jobs
        .start(kind, move |context| run_read(&app, &ids, context))
        .map_err(|error| {
            tracing::error!(%error, "cannot start reading the files' metadata");
            FileMetadataStartFailure::JobNotStarted
        })?;
    Ok(FileMetadataStarted {
        job_id: job_id.get(),
    })
}

/// Complète les médias d'après la lecture `job_id`, de façon annulable :
/// seulement ce qui manque. Sans lecture en attente (déjà
/// appliquée ou remplacée), ne fait rien.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn apply_file_metadata(
    app: AppHandle,
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    pending: State<'_, PendingFileMetadata>,
    job_id: u32,
) -> Result<EditOutcome, EditFailure> {
    let found = pending.take(job_id);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let Some(found) = found else {
        return Ok(outcome(None, &history));
    };
    let mut catalog = lock(&catalog);
    // Préparé de nouveau : le catalogue a pu changer depuis l'aperçu. Les
    // mots-clés trouvés sont créés dans une première transaction, comme
    // pour une saisie ; ils restent dans la liste après une annulation.
    let transaction = catalog.transaction().map_err(|error| failure(&error))?;
    let edit = FileMetadataEdit::new(&transaction, &found).map_err(|error| failure(&error))?;
    transaction
        .commit()
        .map_err(|error| failure(&CatalogError::from(error)))?;
    let sets_positions = edit.sets_positions();
    let summary = history
        .perform(&mut catalog, Edit::FileMetadata(edit))
        .map_err(|error| failure(&error))?;
    let result = outcome(Some(summary), &history);
    drop(catalog);
    drop(history);
    if sets_positions {
        super::places::request_place_lookup(&app);
    }
    Ok(result)
}

/// Oublie la lecture `job_id` (fenêtre de confirmation fermée sans
/// appliquer).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn discard_file_metadata(pending: State<'_, PendingFileMetadata>, job_id: u32) {
    pending.take(job_id);
}

/// Travail de la tâche de lecture.
fn run_read(
    app: &AppHandle,
    ids: &[MediaId],
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let report = read_files(&catalog, ids, context.cancellation(), |done, total| {
        context.report_progress_of(done, total);
    })
    .map_err(|error| match error {
        ReadError::Cancelled => JobError::Cancelled,
        ReadError::Catalog(error) => catalog_error(&error),
    })?;
    let counts = preview(&catalog, &report.found).map_err(|error| catalog_error(&error))?;
    tracing::info!(
        requested = ids.len(),
        missing = report.missing,
        ?counts,
        "files' metadata read"
    );
    let summary = FileMetadataSummary::new(ids.len(), &report, counts);
    if counts.media > 0 {
        app.state::<PendingFileMetadata>()
            .keep(context.id().get(), report.found);
    }
    Ok(JobResult::FileMetadata { summary })
}

/// Ce que l'application ferait, calculé dans une transaction abandonnée :
/// les mots-clés qu'elle créerait ne sont pas gardés.
fn preview(
    catalog: &Mutex<Catalog>,
    found: &[FoundMetadata],
) -> Result<FileMetadataCounts, CatalogError> {
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let counts = FileMetadataEdit::new(&transaction, found)?.counts();
    // Abandonnée à sa destruction, sans validation : rien n'est écrit.
    drop(transaction);
    Ok(counts)
}

fn catalog_error(error: &CatalogError) -> JobError<JobFailure> {
    tracing::error!(%error, "reading the files' metadata stopped");
    JobError::Failed(JobFailure::CatalogError)
}

impl FileMetadataSummary {
    fn new(requested: usize, report: &ReadReport, counts: FileMetadataCounts) -> Self {
        let count = |value: usize| value as u64;
        Self {
            requested: count(requested),
            missing: report.missing,
            unreadable_metadata: report.unreadable_metadata,
            to_complete: count(counts.media),
            triage: count(counts.triage),
            fields: count(counts.fields),
            keywords: count(counts.keywords),
            gps: count(counts.gps),
            capture_time: count(counts.capture_time),
            details: count(counts.details),
        }
    }
}
