//! Sources de l'écran d'import : disques
//! montés, cartes mémoire et dossiers choisis, puis fichiers d'une source
//! analysée. Les chemins restent dans le cœur : l'interface désigne une
//! source par son numéro et un fichier par sa place dans l'analyse.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::dialogs;
use super::edits::lock;
use super::folder_analysis::{AnalyzeFolderFailure, AnalyzedFolders, start_analysis};
use super::import::ArrangementDto;
use super::import_destination::{self, ChosenDestinations, DestinationChoice};
use super::renaming::RenameExample;
use crate::catalog::Catalog;
use crate::catalog::repositories::recent_import_folders;
use crate::domain::{CaptureTime, CaptureTimeSource, MediaKind, Timestamp};
use crate::import::{Arrangement, SourceFile, SourceFileStatus, group_capture_time, planned_tree};
use crate::platform;
use crate::renaming::{RenameInput, RenameTemplate, render};
use crate::thumbnails::ImportPreviewCache;

/// Sources connues de la session, numérotées dans l'ordre où elles ont été
/// vues (un disque rebranché garde son numéro).
#[derive(Debug, Default)]
pub struct ImportSources(Mutex<Vec<PathBuf>>);

impl ImportSources {
    /// Numéro de `path`, ajouté s'il est nouveau.
    pub fn register(&self, path: &Path) -> u32 {
        let mut paths = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let index = paths
            .iter()
            .position(|known| known == path)
            .unwrap_or_else(|| {
                paths.push(path.to_path_buf());
                paths.len() - 1
            });
        u32::try_from(index + 1).unwrap_or(u32::MAX)
    }

    /// Dossier d'une source.
    #[must_use]
    pub fn path(&self, id: u32) -> Option<PathBuf> {
        let paths = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let index = usize::try_from(id).ok()?.checked_sub(1)?;
        paths.get(index).cloned()
    }
}

/// Nature d'une source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportSourceKind {
    /// Carte mémoire ou appareil photo : copie seulement.
    Card,
    /// Dossier choisi.
    Folder,
}

/// Source proposée dans l'écran d'import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceDto {
    /// Numéro de la source.
    pub id: u32,
    /// Nom affiché.
    pub name: String,
    /// Chemin affiché.
    pub path: String,
    /// Nature.
    pub kind: ImportSourceKind,
    /// Faux si le dossier est introuvable (disque débranché).
    pub available: bool,
}

/// Nom affiché d'une source : le dernier élément de son chemin.
fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn source_dto(sources: &ImportSources, path: &Path) -> ImportSourceDto {
    ImportSourceDto {
        id: sources.register(path),
        name: display_name(path),
        path: path.display().to_string(),
        kind: if platform::is_on_camera_card(path) {
            ImportSourceKind::Card
        } else {
            ImportSourceKind::Folder
        },
        available: path.is_dir(),
    }
}

/// Sources de l'écran d'import : les cartes branchées, et les derniers
/// dossiers choisis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourcesDto {
    /// Cartes mémoire et appareils photo branchés.
    pub cards: Vec<ImportSourceDto>,
    /// Derniers dossiers choisis, du plus récent au plus ancien.
    pub folders: Vec<ImportSourceDto>,
}

/// Cartes branchées et derniers dossiers choisis.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn list_import_sources(app: AppHandle, sources: State<'_, ImportSources>) -> ImportSourcesDto {
    let cards = platform::mounted_volumes()
        .into_iter()
        .filter(|volume| platform::is_on_camera_card(&volume.root))
        .map(|volume| source_dto(&sources, &volume.root))
        .collect();
    let folders = recent_folders(&app)
        .iter()
        .map(|path| source_dto(&sources, path))
        .collect();
    ImportSourcesDto { cards, folders }
}

fn recent_folders(app: &AppHandle) -> Vec<PathBuf> {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return Vec::new();
    };
    recent_import_folders::list(lock(&catalog).connection())
        .inspect_err(|error| tracing::error!(%error, "cannot read the recent import folders"))
        .unwrap_or_default()
}

/// Note `folder` parmi les derniers dossiers choisis (une carte n'y va pas :
/// elle a sa propre liste).
pub fn remember_folder(app: &AppHandle, folder: &Path) {
    if platform::is_on_camera_card(folder) {
        return;
    }
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return;
    };
    if let Err(error) =
        recent_import_folders::touch(lock(&catalog).connection(), folder, Timestamp::now())
    {
        tracing::warn!(%error, "cannot remember the import folder");
    }
}

/// Fait choisir un dossier à importer, qui rejoint les derniers dossiers.
/// `None` si l'utilisateur ferme la fenêtre.
///
/// # Errors
///
/// [`AnalyzeFolderFailure::UnsupportedLocation`] si l'emplacement choisi
/// n'est pas un chemin.
#[tauri::command]
#[specta::specta]
pub async fn choose_import_source(
    app: AppHandle,
    sources: State<'_, ImportSources>,
    dialog_title: String,
) -> Result<Option<ImportSourceDto>, AnalyzeFolderFailure> {
    let start_dir = app.path().picture_dir().ok();
    let Some(folder) = dialogs::pick_folder(&app, dialog_title, start_dir)
        .map_err(|_| AnalyzeFolderFailure::UnsupportedLocation)?
    else {
        return Ok(None);
    };
    remember_folder(&app, &folder);
    Ok(Some(source_dto(&sources, &folder)))
}

/// Retire un dossier des derniers dossiers choisis ; le dossier lui-même
/// n'est pas touché.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn forget_import_source(app: AppHandle, source_id: u32) {
    let path = app.state::<ImportSources>().path(source_id);
    let (Some(path), Some(catalog)) = (path, app.try_state::<Mutex<Catalog>>()) else {
        return;
    };
    if let Err(error) = recent_import_folders::forget(lock(&catalog).connection(), &path) {
        tracing::warn!(%error, "cannot forget the import folder");
    }
}

/// Lance l'analyse d'une source, sous-dossiers compris ou non (choix
/// retenu pour la suite) ; renvoie le numéro de la tâche.
///
/// # Errors
///
/// [`AnalyzeFolderFailure::UnknownSource`] pour un numéro inconnu,
/// [`AnalyzeFolderFailure::JobNotStarted`] si la tâche n'a pas démarré.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn analyze_import_source(
    app: AppHandle,
    sources: State<'_, ImportSources>,
    source_id: u32,
    include_subfolders: bool,
) -> Result<u32, AnalyzeFolderFailure> {
    let folder = sources
        .path(source_id)
        .ok_or(AnalyzeFolderFailure::UnknownSource)?;
    import_destination::remember_include_subfolders(&app, include_subfolders);
    start_analysis(&app, folder, include_subfolders)
}

/// État d'un fichier de la source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportFileStatus {
    /// À importer.
    New,
    /// Déjà au catalogue.
    InCatalog,
    /// Doublon d'une photo du catalogue.
    Duplicate,
}

/// Fichier d'une source analysée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileDto {
    /// Place dans l'analyse (adresse de sa vignette, choix à importer).
    pub index: u32,
    /// Nom du fichier.
    pub name: String,
    /// Sous-dossier dans la source (vide à la racine).
    pub folder: String,
    /// Vrai pour une vidéo.
    pub is_video: bool,
    /// Vrai pour un RAW.
    pub is_raw: bool,
    /// Taille en octets.
    #[specta(type = u32)]
    pub size: u64,
    /// Date de prise de vue locale, `AAAA-MM-JJTHH:MM:SS`.
    pub capture_time: String,
    /// Vrai si la date est celle du fichier (pas de date de prise de vue).
    pub date_from_file: bool,
    /// Groupe : les fichiers d'une paire RAW + JPEG ont le même.
    pub group: u32,
    /// État par rapport au catalogue.
    pub status: ImportFileStatus,
}

fn file_dto(root: &Path, index: usize, source: &SourceFile) -> ImportFileDto {
    let path = &source.file.path;
    let folder = path
        .parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .map(|relative| relative.display().to_string())
        .unwrap_or_default();
    ImportFileDto {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder,
        is_video: source.file.format.kind() == MediaKind::Video,
        is_raw: source.file.format.is_raw(),
        size: source.file.size,
        capture_time: source
            .capture_time
            .local
            .strftime("%Y-%m-%dT%H:%M:%S")
            .to_string(),
        date_from_file: source.capture_time.source
            == crate::domain::CaptureTimeSource::FileModified,
        group: u32::try_from(source.group).unwrap_or(u32::MAX),
        status: match source.status {
            SourceFileStatus::New => ImportFileStatus::New,
            SourceFileStatus::InCatalog => ImportFileStatus::InCatalog,
            SourceFileStatus::Duplicate => ImportFileStatus::Duplicate,
        },
    }
}

/// Raison pour laquelle les fichiers d'une analyse ne sont pas lisibles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnalyzedFilesFailure {
    /// Analyse inconnue, oubliée ou pas encore terminée.
    UnknownAnalysis,
}

/// Fichiers d'une analyse terminée, dans l'ordre des prises de vue.
///
/// # Errors
///
/// [`AnalyzedFilesFailure::UnknownAnalysis`] si l'analyse est inconnue ou
/// pas terminée.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_analyzed_files(
    analyzed: State<'_, AnalyzedFolders>,
    analysis_job_id: u32,
) -> Result<Vec<ImportFileDto>, AnalyzedFilesFailure> {
    let source = analyzed
        .get(analysis_job_id)
        .ok_or(AnalyzedFilesFailure::UnknownAnalysis)?;
    let files = source.files.ok_or(AnalyzedFilesFailure::UnknownAnalysis)?;
    Ok(files
        .iter()
        .enumerate()
        .map(|(index, file)| file_dto(&source.root, index, file))
        .collect())
}

/// Dossier de l'aperçu de l'arborescence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFolderDto {
    /// Chemin relatif à la destination, niveaux séparés par « / » (vide :
    /// la destination elle-même).
    pub path: String,
    /// Nom du dernier niveau.
    pub name: String,
    /// Profondeur (0 : premier niveau).
    pub depth: u32,
    /// Photos et vidéos copiées directement dans ce dossier.
    pub file_count: u32,
    /// Vrai si le dossier sera créé.
    pub is_new: bool,
}

/// Aperçu de l'arborescence d'une copie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DestinationPreviewDto {
    /// Destination (affichée).
    pub destination: String,
    /// Dossiers, parents compris, dans l'ordre de l'arborescence.
    pub folders: Vec<PlannedFolderDto>,
    /// Fichiers cochés sans date de prise de vue, rangés d'après la date du
    /// fichier.
    pub file_date_count: u32,
    /// Renommage : les premiers noms, avant et après.
    pub renamed: Vec<RenameExample>,
}

/// Nombre de noms montrés dans l'aperçu d'un renommage.
const RENAME_EXAMPLES: usize = 3;

/// Copie dont l'aperçu est demandé.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DestinationPreviewRequest {
    /// Fichiers cochés, par leur place dans l'analyse.
    pub selected: Vec<u32>,
    /// Destination.
    pub destination: DestinationChoice,
    /// Rangement.
    pub arrangement: ArrangementDto,
    /// Modèle de renommage, s'il y en a un.
    pub rename: Option<RenameTemplate>,
}

/// Aperçu de l'arborescence (et des premiers noms, pour un renommage) que
/// créerait la copie des fichiers cochés de l'analyse `analysis_job_id`.
/// Rien n'est écrit.
///
/// # Errors
///
/// [`AnalyzedFilesFailure::UnknownAnalysis`] si l'analyse est inconnue ou
/// pas terminée, ou sans destination.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_destination_preview(
    app: AppHandle,
    analyzed: State<'_, AnalyzedFolders>,
    chosen: State<'_, ChosenDestinations>,
    analysis_job_id: u32,
    request: DestinationPreviewRequest,
) -> Result<DestinationPreviewDto, AnalyzedFilesFailure> {
    let DestinationPreviewRequest {
        selected,
        destination,
        arrangement,
        rename,
    } = request;
    let source = analyzed
        .get(analysis_job_id)
        .ok_or(AnalyzedFilesFailure::UnknownAnalysis)?;
    let files = source.files.ok_or(AnalyzedFilesFailure::UnknownAnalysis)?;
    let destination = import_destination::destination(&app, destination, chosen.current())
        .ok_or(AnalyzedFilesFailure::UnknownAnalysis)?;
    let arrangement = Arrangement::from(arrangement);
    let chosen_files: Vec<&SourceFile> = selected
        .iter()
        .filter_map(|&index| files.get(usize::try_from(index).ok()?))
        .collect();
    // Un groupe (paire RAW + JPEG) va dans un même dossier, d'après la
    // première date lue parmi ses fichiers, comme à la copie.
    let mut group_times: HashMap<usize, Vec<CaptureTime>> = HashMap::new();
    for file in &chosen_files {
        group_times
            .entry(file.group)
            .or_default()
            .push(file.capture_time);
    }
    let folders: Vec<PathBuf> = chosen_files
        .iter()
        .map(|file| {
            let capture_time = group_times
                .get(&file.group)
                .and_then(|times| group_capture_time(times))
                .unwrap_or(file.capture_time);
            arrangement.relative_folder(&source.root, &file.file.path, &capture_time)
        })
        .collect();
    let file_date_count = chosen_files
        .iter()
        .filter(|file| file.capture_time.source == CaptureTimeSource::FileModified)
        .count();
    Ok(DestinationPreviewDto {
        destination: destination.display().to_string(),
        folders: planned_tree(&destination, &folders)
            .into_iter()
            .map(|folder| PlannedFolderDto {
                path: folder
                    .path
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
                name: folder
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                depth: u32::try_from(folder.path.components().count().saturating_sub(1))
                    .unwrap_or(u32::MAX),
                file_count: u32::try_from(folder.file_count).unwrap_or(u32::MAX),
                is_new: folder.is_new,
            })
            .collect(),
        file_date_count: u32::try_from(file_date_count).unwrap_or(u32::MAX),
        renamed: rename
            .map(|template| rename_examples(&template, &chosen_files, &group_times))
            .unwrap_or_default(),
    })
}

/// Premiers noms d'un renommage à l'import : un par groupe, numérotés dans
/// l'ordre des fichiers cochés.
fn rename_examples(
    template: &RenameTemplate,
    files: &[&SourceFile],
    group_times: &HashMap<usize, Vec<CaptureTime>>,
) -> Vec<RenameExample> {
    let mut seen = std::collections::HashSet::new();
    files
        .iter()
        .filter(|file| seen.insert(file.group))
        .take(RENAME_EXAMPLES)
        .zip(0_u32..)
        .map(|(file, sequence)| {
            let name = file
                .file
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let capture_time = group_times
                .get(&file.group)
                .and_then(|times| group_capture_time(times))
                .unwrap_or(file.capture_time);
            let stem = render(
                template,
                RenameInput {
                    capture_time: &capture_time,
                    original_name: &name,
                    current_name: &name,
                    camera: file.camera.as_deref(),
                },
                sequence,
            );
            let extension = file
                .file
                .path
                .extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
                .unwrap_or_default();
            RenameExample {
                to: if stem.is_empty() {
                    name.clone()
                } else {
                    format!("{stem}{extension}")
                },
                from: name,
            }
        })
        .collect()
}

/// Oublie une analyse (écran d'import fermé, autre source choisie) et
/// efface ses vignettes.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn forget_analysis(app: AppHandle, analysis_job_id: u32) {
    forget(&app, analysis_job_id);
}

/// Oublie une analyse et efface ses vignettes.
pub fn forget(app: &AppHandle, analysis_job_id: u32) {
    if let Some(analyzed) = app.try_state::<AnalyzedFolders>() {
        analyzed.forget(analysis_job_id);
    }
    if let Some(previews) = app.try_state::<ImportPreviewCache>() {
        previews.forget(analysis_job_id);
    }
}
