//! Import d'un dossier analysé, en tâche de fond : par référence ou par
//! copie (rangée par date), avec au choix un modèle de métadonnées posé sur
//! les fichiers ajoutés.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::edits::lock;
use super::folder_analysis::{AnalyzedFolders, UNREADABLE_EXAMPLES, scan_failure};
use super::import_destination::{
    self, BackupChoice, ChosenDestinations, DestinationChoice, RememberedCopy,
};
use super::import_requests;
use super::import_sources;
use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::catalog::repositories::import_preferences::ImportStacking;
use crate::catalog::repositories::metadata_templates::MetadataTemplate;
use crate::domain::{ImportMode, MetadataTemplateId};
use crate::files::CopyError;
use crate::import::{
    Arrangement, CopyOptions, DestinationLayout, DestinationRefusal, ImportError, ImportOptions,
    ImportReport, import_by_copy, import_by_reference,
};
use crate::jobs::{JobContext, JobError};
use crate::metadata_templates::{self, TemplateError};
use crate::platform;
use crate::renaming::RenameTemplate;
use crate::stacks::BURST_GAP_CHOICES;

/// Bilan d'un import, pour l'interface (compteurs typés `u32` pour
/// TypeScript, envoyés avec leur valeur exacte).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// Photos ajoutées.
    #[specta(type = u32)]
    pub added_photos: u64,
    /// Vidéos ajoutées.
    #[specta(type = u32)]
    pub added_videos: u64,
    /// Fichiers déjà au catalogue.
    #[specta(type = u32)]
    pub already_in_catalog: u64,
    /// Doublons ignorés.
    #[specta(type = u32)]
    pub duplicates_skipped: u64,
    /// Fichiers ajoutés sans date EXIF (date du fichier utilisée).
    #[specta(type = u32)]
    pub without_exif_date: u64,
    /// Fichiers ajoutés dont les métadonnées étaient illisibles.
    #[specta(type = u32)]
    pub unreadable_metadata: u64,
    /// Fichiers ajoutés avec des mots-clés trouvés dans leurs métadonnées.
    #[specta(type = u32)]
    pub with_recovered_keywords: u64,
    /// Fichiers ajoutés dont les sources de métadonnées se contredisaient.
    #[specta(type = u32)]
    pub metadata_conflicts: u64,
    /// Éléments illisibles, non importés.
    #[specta(type = u32)]
    pub unreadable_count: u64,
    /// Quelques éléments illisibles, pour aider l'utilisateur à les trouver.
    pub unreadable_examples: Vec<String>,
    /// Nom du modèle de métadonnées posé sur les fichiers ajoutés.
    pub template_name: Option<String>,
    /// Import par copie : dossier où les fichiers ont été rangés.
    pub destination: Option<String>,
    /// Import par copie : photos et vidéos qui n'ont pas pu être copiées.
    #[specta(type = u32)]
    pub copy_failed_count: u64,
    /// Quelques-unes de ces photos et vidéos.
    pub copy_failed_examples: Vec<String>,
    /// Import par copie : fichiers annexes (XMP, `.dop`) non copiés.
    #[specta(type = u32)]
    pub sidecars_not_copied_count: u64,
    /// Copie de sauvegarde : dossier où les originaux ont été copiés.
    pub backup_folder: Option<String>,
    /// Copie de sauvegarde : fichiers qui n'ont pas pu y être copiés.
    #[specta(type = u32)]
    pub backup_failed_count: u64,
    /// Quelques-uns de ces fichiers.
    pub backup_failed_examples: Vec<String>,
}

impl From<&ImportReport> for ImportSummary {
    fn from(report: &ImportReport) -> Self {
        let counts = report.counts;
        Self {
            added_photos: counts.added_photos,
            added_videos: counts.added_videos,
            already_in_catalog: counts.already_in_catalog,
            duplicates_skipped: counts.duplicates_skipped,
            without_exif_date: counts.without_exif_date,
            unreadable_metadata: counts.unreadable_metadata,
            with_recovered_keywords: counts.with_recovered_keywords,
            metadata_conflicts: counts.metadata_conflicts,
            unreadable_count: report.unreadable.len() as u64,
            unreadable_examples: examples(&report.unreadable),
            template_name: report.template_name.clone(),
            destination: report
                .destination
                .as_ref()
                .map(|path| path.display().to_string()),
            copy_failed_count: report.copy_failed.len() as u64,
            copy_failed_examples: examples(&report.copy_failed),
            sidecars_not_copied_count: report.sidecars_not_copied.len() as u64,
            backup_folder: report
                .backup_folder
                .as_ref()
                .map(|path| path.display().to_string()),
            backup_failed_count: report.backup_failed.len() as u64,
            backup_failed_examples: examples(&report.backup_failed),
        }
    }
}

/// Quelques chemins d'une liste, pour aider l'utilisateur à les trouver.
fn examples(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .take(UNREADABLE_EXAMPLES)
        .map(|path| path.display().to_string())
        .collect()
}

/// Façon d'importer choisie dans la fenêtre d'import.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportPlacementDto {
    /// Laisser les fichiers à leur place.
    Reference,
    /// Copier les fichiers, rangés selon `arrangement`.
    #[serde(rename_all = "camelCase")]
    Copy {
        /// Dossier où les ranger.
        destination: DestinationChoice,
        /// Ajouter le chemin d'origine de chaque fichier à sa légende.
        original_path_in_caption: bool,
        /// Rangement dans ce dossier.
        arrangement: ArrangementDto,
        /// Renommer les copies d'après ce modèle.
        rename: Option<RenameTemplate>,
        /// Copier aussi les originaux dans ce dossier de sauvegarde.
        backup: Option<BackupChoice>,
    },
}

/// Rangement des copies, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArrangementDto {
    /// Organisation.
    pub layout: DestinationLayout,
    /// Sous-dossier de séance ajouté au dernier niveau.
    pub session: Option<String>,
    /// Noms des douze mois, dans la langue de l'interface.
    pub month_names: Vec<String>,
}

impl From<ArrangementDto> for Arrangement {
    fn from(dto: ArrangementDto) -> Self {
        Self {
            layout: dto.layout,
            month_names: dto.month_names,
            session: dto.session.filter(|session| !session.trim().is_empty()),
        }
    }
}

/// Choix faits dans la fenêtre d'import.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportChoicesDto {
    /// Ne pas importer les doublons.
    pub skip_duplicates: bool,
    /// Modèle de métadonnées posé sur les fichiers ajoutés.
    pub template_id: Option<u32>,
    /// Laisser les fichiers à leur place, ou les copier.
    pub placement: ImportPlacementDto,
    /// Empiler les paires RAW + JPEG.
    pub stack_pairs: bool,
    /// Empiler les rafales.
    pub stack_bursts: bool,
    /// Écart maximal entre deux photos d'une rafale, en secondes (une des
    /// valeurs proposées, sinon 2).
    pub burst_gap_seconds: u32,
    /// Fichiers cochés, par leur place dans l'analyse.
    pub selected: Vec<u32>,
}

/// Import démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportStarted {
    /// Identifiant de la tâche, pour suivre sa progression.
    pub job_id: u32,
}

/// Raison pour laquelle l'import n'a pas démarré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportStartFailure {
    /// Cette analyse est inconnue ou a déjà servi à un import.
    UnknownAnalysis,
    /// Aucun fichier n'est coché.
    NothingSelected,
    /// Aucun catalogue n'est ouvert.
    CatalogUnavailable,
    /// Le dossier est sur une carte mémoire ou un appareil photo.
    CameraCard {
        /// Dossier concerné.
        path: String,
    },
    /// Le modèle de métadonnées choisi n'existe plus.
    TemplateNotFound,
    /// La destination est dans le dossier importé, ou l'inverse.
    DestinationOverlap {
        /// Destination choisie.
        destination: String,
    },
    /// La destination est sur une carte mémoire ou un appareil photo.
    DestinationOnCameraCard {
        /// Destination choisie.
        destination: String,
    },
    /// Aucun dossier n'est choisi pour la copie de sauvegarde.
    BackupUnavailable,
    /// La tâche de fond n'a pas pu être lancée.
    JobNotStarted,
}

/// Lance l'import des fichiers cochés d'une analyse terminée, par
/// référence ou par copie selon `placement`. Le modèle `template_id` (ou aucun) est lu
/// maintenant ; lui et les autres choix sont retenus pour être
/// présélectionnés à l'import suivant.
///
/// # Errors
///
/// Renvoie la raison pour laquelle l'import n'a pas démarré.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn import_analyzed_folder(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    analyzed: State<'_, AnalyzedFolders>,
    chosen: State<'_, ChosenDestinations>,
    analysis_job_id: u32,
    choices: ImportChoicesDto,
) -> Result<ImportStarted, ImportStartFailure> {
    let ImportChoicesDto {
        skip_duplicates,
        template_id,
        placement,
        stack_pairs,
        stack_bursts,
        burst_gap_seconds,
        selected,
    } = choices;
    let stacking = ImportStacking {
        pairs: stack_pairs,
        bursts: stack_bursts,
        burst_gap_seconds: if BURST_GAP_CHOICES.contains(&burst_gap_seconds) {
            burst_gap_seconds
        } else {
            ImportStacking::default().burst_gap_seconds
        },
    };
    let source = analyzed
        .get(analysis_job_id)
        .ok_or(ImportStartFailure::UnknownAnalysis)?;
    let files = source
        .files
        .as_ref()
        .ok_or(ImportStartFailure::UnknownAnalysis)?;
    let only: HashSet<PathBuf> = selected
        .iter()
        .filter_map(|&index| files.get(usize::try_from(index).ok()?))
        .map(|file| file.file.path.clone())
        .collect();
    if only.is_empty() {
        return Err(ImportStartFailure::NothingSelected);
    }
    let folder = source.root;
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(ImportStartFailure::CatalogUnavailable)?;
    let copy = copy_options(&app, &folder, placement, &chosen)?;
    let template = import_template(&catalog, template_id)?;
    import_destination::remember_choices(
        &app,
        if copy.is_some() {
            ImportMode::Copy
        } else {
            ImportMode::Reference
        },
        copy.as_ref().map(|copy| RememberedCopy {
            destination: copy.destination.clone(),
            original_path_in_caption: copy.original_path_in_caption,
            layout: copy.arrangement.layout.clone(),
            rename: copy.rename.clone(),
            backup: copy.backup.clone(),
        }),
        stacking,
    );

    let options = ImportOptions {
        skip_duplicates,
        template,
        stack_pairs: stacking.pairs,
        burst_gap_seconds: stacking.bursts.then_some(stacking.burst_gap_seconds),
        only: Some(only),
        scan: source.scan.clone(),
        ..ImportOptions::default()
    };
    let card = platform::camera_card_root(&folder);
    let kind = JobKind::Import {
        folder: folder.display().to_string(),
        copy: copy.is_some(),
        card: card.as_deref().map(import_requests::volume_name),
    };
    // L'analyse a servi : l'écran d'import se ferme.
    import_sources::forget(&app, analysis_job_id);
    chosen.clear();
    let app_handle = app.clone();
    let job_id = jobs
        .start(kind, move |context| {
            run_import(&app, &folder, copy.as_ref(), &options, context)
        })
        .map_err(|error| {
            tracing::error!(%error, "cannot start the import");
            ImportStartFailure::JobNotStarted
        })?;
    if let Some(root) = card {
        import_requests::remember_card(&app_handle, job_id.get(), root);
    }
    Ok(ImportStarted {
        job_id: job_id.get(),
    })
}

/// Lance l'import par référence de `folder` (synchronisation d'un dossier) : les fichiers déjà au catalogue sont ignorés ; les
/// paires et rafales sont empilées selon les choix retenus.
pub(super) fn start_reference_import(
    app: AppHandle,
    jobs: &AppJobManager,
    folder: PathBuf,
    stacking: ImportStacking,
) -> Result<ImportStarted, ImportStartFailure> {
    if platform::is_on_camera_card(&folder) {
        return Err(ImportStartFailure::CameraCard {
            path: folder.display().to_string(),
        });
    }
    let options = ImportOptions {
        stack_pairs: stacking.pairs,
        burst_gap_seconds: stacking.bursts.then_some(stacking.burst_gap_seconds),
        ..ImportOptions::default()
    };
    let kind = JobKind::Import {
        folder: folder.display().to_string(),
        copy: false,
        card: None,
    };
    let job_id = jobs
        .start(kind, move |context| {
            run_import(&app, &folder, None, &options, context)
        })
        .map_err(|error| {
            tracing::error!(%error, "cannot start the import");
            ImportStartFailure::JobNotStarted
        })?;
    Ok(ImportStarted {
        job_id: job_id.get(),
    })
}

/// Réglages de la copie choisie (`None` : fichiers laissés à leur place),
/// après avoir vérifié que la source et la destination le permettent.
fn copy_options(
    app: &AppHandle,
    folder: &Path,
    placement: ImportPlacementDto,
    chosen: &ChosenDestinations,
) -> Result<Option<CopyOptions>, ImportStartFailure> {
    match placement {
        ImportPlacementDto::Reference => {
            if platform::is_on_camera_card(folder) {
                return Err(ImportStartFailure::CameraCard {
                    path: folder.display().to_string(),
                });
            }
            Ok(None)
        }
        ImportPlacementDto::Copy {
            destination,
            original_path_in_caption,
            arrangement,
            rename,
            backup,
        } => {
            let destination = import_destination::destination(app, destination, chosen.current())
                .ok_or(ImportStartFailure::CatalogUnavailable)?;
            crate::import::check_destination(folder, &destination).map_err(destination_refused)?;
            let backup = match backup {
                Some(choice) => {
                    let backup = import_destination::backup_destination(
                        app,
                        choice,
                        chosen.current_backup(),
                    )
                    .ok_or(ImportStartFailure::BackupUnavailable)?;
                    crate::import::check_destination(folder, &backup)
                        .map_err(destination_refused)?;
                    Some(backup)
                }
                None => None,
            };
            Ok(Some(CopyOptions {
                destination,
                original_path_in_caption,
                arrangement: arrangement.into(),
                rename: rename.filter(|template| !template.parts.is_empty()),
                backup,
            }))
        }
    }
}

fn destination_refused(refusal: DestinationRefusal) -> ImportStartFailure {
    match refusal {
        DestinationRefusal::Overlap { destination, .. } => ImportStartFailure::DestinationOverlap {
            destination: destination.display().to_string(),
        },
        DestinationRefusal::OnCameraCard { destination } => {
            ImportStartFailure::DestinationOnCameraCard {
                destination: destination.display().to_string(),
            }
        }
    }
}

/// Lit le modèle choisi pour l'import et le retient pour le suivant.
fn import_template(
    catalog: &Mutex<Catalog>,
    template_id: Option<u32>,
) -> Result<Option<MetadataTemplate>, ImportStartFailure> {
    let unavailable = |error: &dyn std::fmt::Display| {
        tracing::error!(%error, "cannot read the import template");
        ImportStartFailure::CatalogUnavailable
    };
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction().map_err(|error| unavailable(&error))?;
    let id = template_id.map(|id| MetadataTemplateId::new(i64::from(id)));
    let template =
        metadata_templates::choose_for_import(&transaction, id).map_err(|error| match error {
            TemplateError::NotFound => ImportStartFailure::TemplateNotFound,
            // Lire ou retenir un modèle ne peut échouer autrement.
            other => unavailable(&other),
        })?;
    transaction.commit().map_err(|error| unavailable(&error))?;
    Ok(template)
}

/// Travail de la tâche d'import.
fn run_import(
    app: &AppHandle,
    folder: &Path,
    copy: Option<&CopyOptions>,
    options: &ImportOptions,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let time_zone = TimeZone::system();
    let progress = |done, total| context.report_progress_of(done, total);
    let report = match copy {
        Some(copy) => import_by_copy(
            &catalog,
            folder,
            copy,
            options,
            &time_zone,
            context.cancellation(),
            progress,
        ),
        None => import_by_reference(
            &catalog,
            folder,
            options,
            &time_zone,
            context.cancellation(),
            progress,
        ),
    }
    .inspect(|_| request_background_passes(app))
    .inspect_err(|error| {
        // Un import annulé ou interrompu a pu ajouter des photos.
        if matches!(
            error,
            ImportError::Cancelled | ImportError::Catalog(_) | ImportError::Destination(_)
        ) {
            request_background_passes(app);
        }
    })
    .map_err(|error| {
        tracing::warn!(%error, "import stopped");
        match error {
            ImportError::Cancelled => JobError::Cancelled,
            ImportError::CameraCard { path } => JobError::Failed(JobFailure::CameraCard {
                path: path.display().to_string(),
            }),
            ImportError::Source(scan_error) => scan_failure(scan_error),
            ImportError::DestinationRefused(refusal) => {
                JobError::Failed(JobFailure::DestinationUnavailable {
                    path: match refusal {
                        DestinationRefusal::Overlap { destination, .. }
                        | DestinationRefusal::OnCameraCard { destination } => {
                            destination.display().to_string()
                        }
                    },
                })
            }
            ImportError::Destination(error) => destination_failure(&error),
            ImportError::Catalog(_) => JobError::Failed(JobFailure::CatalogError),
        }
    })?;
    Ok(JobResult::Import {
        summary: ImportSummary::from(&report),
    })
}

/// Traduit l'arrêt d'une copie faute de pouvoir écrire dans la destination.
fn destination_failure(error: &CopyError) -> JobError<JobFailure> {
    let path = match error {
        CopyError::Destination { path, .. } => path.display().to_string(),
        _ => String::new(),
    };
    JobError::Failed(if error.is_storage_full() {
        JobFailure::DestinationFull { path }
    } else {
        JobFailure::DestinationUnavailable { path }
    })
}

/// Tâches de fond à relancer pour les photos qui viennent d'arriver :
/// vignettes, et lieu de celles qui sont géolocalisées.
fn request_background_passes(app: &AppHandle) {
    super::thumbnails::request_thumbnails(app);
    super::places::request_place_lookup(app);
}
