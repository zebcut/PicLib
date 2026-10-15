//! Parcours d'un dossier à la recherche de photos et de vidéos.
//!
//! **Lecture seule stricte** : les fichiers ne sont jamais ouverts ; seuls
//! leur nom, leur taille et leur date de modification sont lus dans le
//! système de fichiers.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use crate::domain::{MediaFormat, MediaKind, Timestamp};
use crate::jobs::CancellationToken;

/// Nombre d'éléments parcourus entre deux annonces de progression.
const PROGRESS_STEP: u64 = 256;

/// Extensions des bibliothèques d'autres logiciels (Photos, Lightroom...) :
/// ce sont des dossiers dont le contenu interne ne doit pas être importé
/// fichier par fichier.
const FOREIGN_LIBRARY_EXTENSIONS: &[&str] = &[
    "photoslibrary",
    "migratedphotolibrary",
    "photolibrary",
    "aplibrary",
    "lrlibrary",
    "lrdata",
];

/// Extensions des fichiers annexes : XMP, et `.dop` de DxO PhotoLab.
pub const SIDECAR_EXTENSIONS: [&str; 2] = ["xmp", "dop"];

/// Photo ou vidéo trouvée lors du parcours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Chemin complet du fichier.
    pub path: PathBuf,
    /// Format reconnu d'après l'extension.
    pub format: MediaFormat,
    /// Taille en octets.
    pub size: u64,
    /// Date de dernière modification.
    pub modified: Timestamp,
}

/// Élément qui n'a pas pu être lu (droits insuffisants, disque débranché...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanIssue {
    /// Fichier ou dossier concerné.
    pub path: PathBuf,
    /// Cause, pour le journal.
    pub reason: String,
}

/// Résultat du parcours d'un dossier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Photos et vidéos trouvées.
    pub files: Vec<ScannedFile>,
    /// Nombre de dossiers parcourus, dossier de départ compris.
    pub folder_count: u64,
    /// Fichiers d'un autre type (textes, XMP...), liens symboliques.
    pub ignored_count: u64,
    /// Fichiers annexes XMP et `.dop` (comptés aussi dans `ignored_count` :
    /// ils ne sont pas importés, mais leurs métadonnées sont lues).
    pub sidecars: Vec<PathBuf>,
    /// Éléments illisibles, qui n'ont pas empêché de parcourir le reste.
    pub issues: Vec<ScanIssue>,
}

/// Chiffres clés d'un parcours, pour le résumé montré à l'utilisateur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Nombre de photos, RAW compris.
    pub photo_count: u64,
    /// Nombre de photos RAW (inclus dans `photo_count`).
    pub raw_count: u64,
    /// Nombre de vidéos.
    pub video_count: u64,
    /// Taille totale des photos et vidéos, en octets.
    pub total_size: u64,
}

impl ScanReport {
    /// Compte les photos, RAW et vidéos trouvées, et leur taille totale.
    #[must_use]
    pub fn summary(&self) -> ScanSummary {
        self.files
            .iter()
            .fold(ScanSummary::default(), |mut summary, file| {
                match file.format.kind() {
                    MediaKind::Photo => summary.photo_count += 1,
                    MediaKind::Video => summary.video_count += 1,
                }
                if file.format.is_raw() {
                    summary.raw_count += 1;
                }
                summary.total_size = summary.total_size.saturating_add(file.size);
                summary
            })
    }
}

/// Erreur empêchant tout parcours.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// Le dossier de départ n'existe pas ou n'est pas lisible.
    #[error("cannot read {}: {source}", path.display())]
    SourceUnavailable {
        /// Dossier demandé.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// Le chemin désigne un fichier, pas un dossier.
    #[error("{} is not a folder", path.display())]
    NotAFolder {
        /// Chemin demandé.
        path: PathBuf,
    },
    /// L'utilisateur a annulé le parcours.
    #[error("scan cancelled")]
    Cancelled,
}

/// Parcourt `root` et tous ses sous-dossiers à la recherche de photos et de
/// vidéos prises en charge.
///
/// - Les liens symboliques ne sont pas suivis (pas de boucle possible, pas de
///   sortie du dossier choisi).
/// - Les éléments cachés (nom commençant par un point) et les bibliothèques
///   d'autres logiciels sont ignorés, avec tout leur contenu.
/// - Un élément illisible est noté dans le rapport sans interrompre le reste.
///
/// `on_progress` reçoit régulièrement le nombre de photos et vidéos trouvées.
///
/// # Errors
///
/// Échoue si `root` est introuvable, illisible ou n'est pas un dossier, ou si
/// `cancellation` est déclenché.
pub fn scan_folder(
    root: &Path,
    cancellation: &CancellationToken,
    on_progress: impl FnMut(u64),
) -> Result<ScanReport, ScanError> {
    walk(root, None, &[], cancellation, on_progress)
}

/// Parcours d'une source d'import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceScan {
    /// Parcourir aussi les sous-dossiers ; sinon le dossier seul.
    pub include_subfolders: bool,
    /// Dossiers ignorés avec tout leur contenu (caches et sauvegardes du
    /// catalogue, qui ne sont pas des photos de l'utilisateur).
    pub excluded: Vec<PathBuf>,
}

impl Default for SourceScan {
    fn default() -> Self {
        Self {
            include_subfolders: true,
            excluded: Vec::new(),
        }
    }
}

/// Parcourt une source d'import, comme [`scan_folder`], avec ou sans ses
/// sous-dossiers et sans les dossiers exclus.
///
/// # Errors
///
/// Comme [`scan_folder`].
pub fn scan_source(
    root: &Path,
    options: &SourceScan,
    cancellation: &CancellationToken,
    on_progress: impl FnMut(u64),
) -> Result<ScanReport, ScanError> {
    let depth = (!options.include_subfolders).then_some(1);
    walk(root, depth, &options.excluded, cancellation, on_progress)
}

/// Photos, vidéos et fichiers annexes de `folder` seul, sans ses
/// sous-dossiers (relecture des métadonnées de photos déjà au catalogue). Mêmes règles que [`scan_folder`].
///
/// # Errors
///
/// Échoue si `folder` est introuvable, illisible ou n'est pas un dossier.
pub fn list_folder(folder: &Path) -> Result<ScanReport, ScanError> {
    walk(folder, Some(1), &[], &CancellationToken::default(), |_| {})
}

/// Parcours commun : tous les niveaux, ou `max_depth` au plus (1 : le
/// contenu du dossier seul), sans les dossiers `excluded`.
fn walk(
    root: &Path,
    max_depth: Option<usize>,
    excluded: &[PathBuf],
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64),
) -> Result<ScanReport, ScanError> {
    let metadata = std::fs::metadata(root).map_err(|source| ScanError::SourceUnavailable {
        path: root.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(ScanError::NotAFolder {
            path: root.to_path_buf(),
        });
    }

    let mut report = ScanReport::default();
    let mut walker = WalkDir::new(root).follow_links(false);
    if let Some(depth) = max_depth {
        walker = walker.max_depth(depth);
    }
    let entries = walker.into_iter().filter_entry(|entry| {
        entry.depth() == 0
            || !(is_skipped(entry)
                || (entry.file_type().is_dir() && excluded.iter().any(|dir| dir == entry.path())))
    });

    for (visited, entry) in (1_u64..).zip(entries) {
        if cancellation.is_cancelled() {
            return Err(ScanError::Cancelled);
        }
        match entry {
            Ok(entry) => record_entry(&entry, &mut report),
            Err(error) if error.depth() == 0 => {
                return Err(ScanError::SourceUnavailable {
                    path: root.to_path_buf(),
                    source: error
                        .into_io_error()
                        .unwrap_or_else(|| std::io::Error::other("unreadable folder")),
                });
            }
            Err(error) => report.issues.push(ScanIssue {
                path: error.path().unwrap_or(root).to_path_buf(),
                reason: error.to_string(),
            }),
        }
        if visited % PROGRESS_STEP == 0 {
            on_progress(report.files.len() as u64);
        }
    }

    on_progress(report.files.len() as u64);
    Ok(report)
}

/// Ajoute un élément au rapport selon sa nature.
fn record_entry(entry: &DirEntry, report: &mut ScanReport) {
    let file_type = entry.file_type();
    if file_type.is_dir() {
        report.folder_count += 1;
        return;
    }
    let format = if file_type.is_file() {
        MediaFormat::from_path(entry.path())
    } else {
        None
    };
    let Some(format) = format else {
        report.ignored_count += 1;
        if file_type.is_file() && is_sidecar(entry.path()) {
            report.sidecars.push(entry.path().to_path_buf());
        }
        return;
    };
    match entry.metadata() {
        Ok(metadata) => report.files.push(ScannedFile {
            path: entry.path().to_path_buf(),
            format,
            size: metadata.len(),
            modified: metadata
                .modified()
                .map_or(Timestamp::from_unix_millis(0), Timestamp::from_system_time),
        }),
        Err(error) => report.issues.push(ScanIssue {
            path: entry.path().to_path_buf(),
            reason: error.to_string(),
        }),
    }
}

/// Élément à ignorer avec tout son contenu : caché, ou bibliothèque d'un
/// autre logiciel.
fn is_skipped(entry: &DirEntry) -> bool {
    let name = entry.file_name();
    is_hidden(name) || (entry.file_type().is_dir() && is_foreign_library(entry.path()))
}

/// Fichier annexe (`photo.xmp`, `photo.NEF.xmp`, `photo.NEF.dop`).
fn is_sidecar(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            SIDECAR_EXTENSIONS
                .iter()
                .any(|sidecar| extension.eq_ignore_ascii_case(sidecar))
        })
}

fn is_hidden(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

fn is_foreign_library(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            FOREIGN_LIBRARY_EXTENSIONS
                .iter()
                .any(|library| extension.eq_ignore_ascii_case(library))
        })
}
