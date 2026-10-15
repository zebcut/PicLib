//! Contenu d'une source d'import, montré dans l'écran d'import avant de
//! rien importer : chaque photo ou vidéo avec sa
//! date de prise de vue, son groupe (paire RAW + JPEG) et son état (nouvelle,
//! déjà au catalogue, doublon). Lecture seule : la source n'est jamais
//! modifiée.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::tz::TimeZone;

use super::copy::group_key;
use super::recovery;
use super::sidecars::Sidecars;
use crate::catalog::repositories::{folders, media};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{CaptureTime, duplicate_name_key};
use crate::files::{ScanReport, ScannedFile};
use crate::jobs::CancellationToken;
use crate::metadata::{ExtractedMetadata, read_metadata};

/// Nombre de fichiers comparés au catalogue par verrou : l'interface reste
/// libre d'y lire entre deux lots.
const CATALOG_BATCH: usize = 200;

/// État d'un fichier de la source par rapport au catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFileStatus {
    /// À importer.
    New,
    /// Ce fichier même est déjà au catalogue (import par référence passé).
    InCatalog,
    /// Une photo de même nom d'origine, même date et même taille est au
    /// catalogue.
    Duplicate,
}

/// Photo ou vidéo de la source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Fichier trouvé.
    pub file: ScannedFile,
    /// Date de prise de vue (à défaut, celle du fichier).
    pub capture_time: CaptureTime,
    /// Numéro du groupe : les fichiers d'un même nom dans un même dossier
    /// (une paire RAW + JPEG) ont le même.
    pub group: usize,
    /// État par rapport au catalogue.
    pub status: SourceFileStatus,
    /// Modèle de l'appareil (à défaut, sa marque), pour l'aperçu d'un
    /// renommage.
    pub camera: Option<String>,
}

/// Lit la date de prise de vue de chaque photo et vidéo de `scan`, la
/// compare au catalogue, et renvoie les fichiers dans l'ordre des prises
/// de vue (les fichiers d'un groupe côte à côte). `on_progress` reçoit le
/// nombre de fichiers lus et le total.
///
/// # Errors
///
/// `Ok(None)` si l'utilisateur annule ; une erreur si le catalogue est
/// illisible.
pub fn read_source(
    catalog: &Mutex<Catalog>,
    scan: &ScanReport,
    time_zone: &TimeZone,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Option<Vec<SourceFile>>, CatalogError> {
    let sidecars = Sidecars::new(&scan.files, &scan.sidecars);
    let total = scan.files.len() as u64;
    let mut groups: HashMap<(PathBuf, String), usize> = HashMap::new();
    let mut next_group = 0;
    let mut files = Vec::with_capacity(scan.files.len());
    for chunk in scan.files.chunks(CATALOG_BATCH) {
        let mut read = Vec::with_capacity(chunk.len());
        for file in chunk {
            if cancellation.is_cancelled() {
                return Ok(None);
            }
            let group = if let Some(key) = group_key(&file.path) {
                let next = next_group;
                *groups.entry(key).or_insert_with(|| {
                    next_group += 1;
                    next
                })
            } else {
                next_group += 1;
                next_group - 1
            };
            let (capture_time, camera) = capture_time(file, &sidecars, time_zone);
            read.push((file, capture_time, camera, group));
        }
        let catalog = lock(catalog);
        let connection = catalog.connection();
        for (file, capture_time, camera, group) in read {
            let status = status(connection, file, &capture_time)?;
            files.push(SourceFile {
                file: file.clone(),
                capture_time,
                group,
                status,
                camera,
            });
        }
        drop(catalog);
        on_progress(files.len() as u64, total);
    }
    files.sort_by(|a, b| {
        (a.capture_time.local, a.group, &a.file.path).cmp(&(
            b.capture_time.local,
            b.group,
            &b.file.path,
        ))
    });
    Ok(Some(order_groups(files)))
}

/// Place les fichiers d'un groupe juste après le premier d'entre eux
/// (un JPEG sans date suit son RAW).
fn order_groups(files: Vec<SourceFile>) -> Vec<SourceFile> {
    let mut positions: HashMap<usize, usize> = HashMap::new();
    let mut ordered: Vec<Vec<SourceFile>> = Vec::new();
    for file in files {
        if let Some(&position) = positions.get(&file.group) {
            ordered[position].push(file);
        } else {
            positions.insert(file.group, ordered.len());
            ordered.push(vec![file]);
        }
    }
    ordered.into_iter().flatten().collect()
}

/// Date de prise de vue d'un fichier, comme à l'import : celle de l'EXIF,
/// à défaut celle des fichiers annexes, à défaut celle du fichier ; et son
/// appareil.
fn capture_time(
    file: &ScannedFile,
    sidecars: &Sidecars,
    time_zone: &TimeZone,
) -> (CaptureTime, Option<String>) {
    let metadata = read_metadata(&file.path, file.format).unwrap_or_else(|error| {
        tracing::debug!(path = %file.path.display(), %error, "no metadata for the import preview");
        ExtractedMetadata::default()
    });
    let capture_time = metadata
        .capture_time
        .or_else(|| {
            recovery::recover(file, &metadata, sidecars)
                .metadata
                .capture_time
        })
        .unwrap_or_else(|| CaptureTime::from_file_modified(file.modified, time_zone));
    let details = metadata.details;
    (capture_time, details.camera_model.or(details.camera_make))
}

/// Compare un fichier au catalogue : lui-même déjà inscrit, ou un doublon.
fn status(
    connection: &rusqlite::Connection,
    file: &ScannedFile,
    capture_time: &CaptureTime,
) -> Result<SourceFileStatus, CatalogError> {
    let name = file
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(parent) = file.path.parent()
        && let Some(folder) = folders::find_by_path(connection, parent)?
        && media::exists_in_folder(connection, folder.id, &name)?
    {
        return Ok(SourceFileStatus::InCatalog);
    }
    if media::find_duplicate(
        connection,
        &duplicate_name_key(&name),
        file.size,
        capture_time,
    )?
    .is_some()
    {
        return Ok(SourceFileStatus::Duplicate);
    }
    Ok(SourceFileStatus::New)
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
