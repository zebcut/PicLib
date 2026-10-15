//! Tâches de fond vues par l'interface : abonnement aux mises à jour et
//! annulation.
//!
//! L'interface s'abonne une fois au démarrage en ouvrant un canal dédié
//! (« Channel » Tauri) : elle y reçoit l'état de chaque tâche, sans avoir
//! besoin de permission supplémentaire.

use serde::Serialize;
use specta::Type;
use tauri::State;
use tauri::ipc::Channel;

use super::deletion::TrashSummary;
use super::editor_watch::EditedVersionsSummary;
use super::file_metadata::FileMetadataSummary;
use super::folder_analysis::FolderAnalysisSummary;
use super::import::ImportSummary;
use super::missing::MissingSummary;
use super::places::PlacesSummary;
use super::thumbnails::ThumbnailsSummary;
use super::xmp::XmpWriteSummary;
use crate::jobs::{JobId, JobManager, JobSnapshot, JobStatus};

/// Gestionnaire des tâches de fond de l'application.
pub type AppJobManager = JobManager<JobKind, JobResult, JobFailure>;

/// Nature d'une tâche et ses paramètres, pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobKind {
    /// Analyse d'un dossier avant import.
    FolderAnalysis {
        /// Dossier analysé.
        folder: String,
    },
    /// Import d'un dossier.
    Import {
        /// Dossier importé.
        folder: String,
        /// Vrai pour un import par copie.
        copy: bool,
        /// Nom de la carte mémoire importée, que le bilan propose d'éjecter.
        card: Option<String>,
    },
    /// Fabrication des vignettes manquantes.
    Thumbnails,
    /// Remplissage du lieu des photos géolocalisées.
    Places,
    /// Mise à la corbeille des photos rejetées.
    Trash {
        /// Nombre de fichiers à placer dans la corbeille.
        count: u32,
    },
    /// Lecture des métadonnées des fichiers de médias du catalogue.
    FileMetadata {
        /// Nombre de médias dont les fichiers sont relus.
        count: u32,
    },
    /// Recherche des fichiers manquants.
    MissingFiles,
    /// Écriture des métadonnées dans les fichiers XMP.
    XmpWrite {
        /// Nombre de médias dont le XMP est écrit.
        count: u32,
        /// Écriture automatique, après des modifications :
        /// discrète.
        automatic: bool,
    },
    /// Ajout des photos traitées par un éditeur externe.
    EditedVersions {
        /// Fichiers reconnus.
        count: u32,
    },
}

/// Résultat d'une tâche terminée avec succès.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobResult {
    /// Résultat d'une analyse de dossier.
    FolderAnalysis {
        /// Chiffres clés.
        summary: FolderAnalysisSummary,
    },
    /// Bilan d'un import.
    Import {
        /// Chiffres clés.
        summary: ImportSummary,
    },
    /// Bilan d'une fabrication de vignettes.
    Thumbnails {
        /// Chiffres clés.
        summary: ThumbnailsSummary,
    },
    /// Bilan d'un remplissage des lieux.
    Places {
        /// Chiffres clés.
        summary: PlacesSummary,
    },
    /// Bilan d'une mise à la corbeille.
    Trash {
        /// Chiffres clés.
        summary: TrashSummary,
    },
    /// Ce que la lecture des métadonnées des fichiers compléterait.
    FileMetadata {
        /// Chiffres clés.
        summary: FileMetadataSummary,
    },
    /// Bilan d'une recherche des fichiers manquants.
    MissingFiles {
        /// Chiffres clés.
        summary: MissingSummary,
    },
    /// Bilan d'une écriture des XMP.
    XmpWrite {
        /// Chiffres clés.
        summary: XmpWriteSummary,
    },
    /// Photos traitées ajoutées au catalogue.
    EditedVersions {
        /// Chiffres clés.
        summary: EditedVersionsSummary,
    },
}

/// Échec prévu d'une tâche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobFailure {
    /// Le dossier n'existe plus ou n'est pas lisible.
    SourceUnavailable {
        /// Dossier concerné.
        path: String,
    },
    /// Le chemin désigne un fichier, pas un dossier.
    NotAFolder {
        /// Chemin concerné.
        path: String,
    },
    /// Le dossier est sur une carte mémoire ou un appareil photo : import par
    /// référence refusé.
    CameraCard {
        /// Dossier concerné.
        path: String,
    },
    /// Import par copie : la destination est pleine.
    DestinationFull {
        /// Dossier concerné.
        path: String,
    },
    /// Import par copie : la destination ne peut pas être écrite (droits,
    /// disque débranché...).
    DestinationUnavailable {
        /// Dossier concerné.
        path: String,
    },
    /// Le catalogue n'a pas pu être lu ou écrit.
    CatalogError,
}

/// État d'une tâche, tel que reçu par l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum JobUpdateStatus {
    /// En cours.
    Running {
        /// Nombre d'éléments traités.
        #[specta(type = u32)]
        progress: u64,
        /// Nombre total d'éléments à traiter, s'il est connu.
        #[specta(type = Option<u32>)]
        total: Option<u64>,
    },
    /// Terminée avec succès.
    Completed {
        /// Résultat (en boîte : un bilan d'import est bien plus gros que les
        /// autres états).
        result: Box<JobResult>,
    },
    /// Terminée sur une erreur prévue.
    Failed {
        /// Cause.
        failure: JobFailure,
    },
    /// Annulée par l'utilisateur.
    Cancelled,
    /// Interrompue par une erreur interne.
    Crashed,
}

/// Mise à jour d'une tâche, envoyée à l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct JobUpdate {
    /// Identifiant de la tâche.
    pub id: u32,
    /// Nature de la tâche.
    pub kind: JobKind,
    /// État actuel.
    pub status: JobUpdateStatus,
}

impl From<&JobSnapshot<JobKind, JobResult, JobFailure>> for JobUpdate {
    fn from(snapshot: &JobSnapshot<JobKind, JobResult, JobFailure>) -> Self {
        let status = match &snapshot.status {
            JobStatus::Running { progress, total } => JobUpdateStatus::Running {
                progress: *progress,
                total: *total,
            },
            JobStatus::Completed(result) => JobUpdateStatus::Completed {
                result: Box::new(result.clone()),
            },
            JobStatus::Failed(failure) => JobUpdateStatus::Failed {
                failure: failure.clone(),
            },
            JobStatus::Cancelled => JobUpdateStatus::Cancelled,
            JobStatus::Crashed => JobUpdateStatus::Crashed,
        };
        Self {
            id: snapshot.id.get(),
            kind: snapshot.kind.clone(),
            status,
        }
    }
}

/// Abonne l'interface aux mises à jour des tâches de fond. Elle reçoit
/// aussitôt l'état des tâches en cours.
// `State` et `Channel` sont fournis par Tauri et doivent être reçus par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn subscribe_jobs(jobs: State<'_, AppJobManager>, on_update: Channel<JobUpdate>) {
    tracing::debug!("interface subscribed to job updates");
    // Si l'envoi échoue (fenêtre fermée ou rechargée), l'abonnement est
    // abandonné.
    jobs.subscribe(move |snapshot| on_update.send(JobUpdate::from(snapshot)).is_ok());
}

/// Demande l'annulation d'une tâche. Renvoie `false` si elle est déjà
/// terminée.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn cancel_job(jobs: State<'_, AppJobManager>, job_id: u32) -> bool {
    jobs.cancel(JobId::new(job_id))
}
