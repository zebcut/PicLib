//! Écriture des métadonnées du catalogue dans les fichiers annexes XMP
//!.
//!
//! Pour chaque média : son fichier XMP est choisi (`photo.xmp` pour un RAW,
//! nom d'Adobe ; `photo.JPG.xmp` pour les autres formats, qui ne se mélange
//! jamais avec un RAW du même nom), relu s'il existe, fusionné avec les
//! valeurs du catalogue (`metadata::xmp::write_packet`), puis écrit par
//! `files::write_sidecar`. Le catalogue n'est verrouillé que pour lire les
//! médias d'un lot et noter les résultats ; les fichiers sont lus et écrits
//! hors du verrou.
//!
//! Garde-fous : rien n'est écrit pour une photo introuvable, ni sur une
//! carte mémoire ; un fichier modifié par un autre
//! logiciel depuis la dernière écriture de PicLib n'est remplacé que sur
//! demande (`overwrite_changed`) ; un fichier que PicLib n'a pas écrit est
//! d'abord copié dans le dossier de travail (`XMP d'origine`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::catalog::repositories::xmp_sync::{self, XmpError as WriteFailure, XmpSource};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaId, MediaKind, Orientation, Rotation};
use crate::files::{self, FileStamp, SidecarWriteError};
use crate::jobs::CancellationToken;
use crate::metadata::xmp::{self, XmpContent, XmpError};
use crate::platform;

/// Médias traités entre deux passages au catalogue.
const BATCH_SIZE: usize = 100;

/// Bilan d'une écriture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteReport {
    /// Fichiers écrits.
    pub written: u64,
    /// Fichiers modifiés par un autre logiciel, laissés tels quels.
    pub changed_elsewhere: u64,
    /// Échecs (photo introuvable, carte mémoire, fichier illisible...).
    pub failed: u64,
}

/// Erreur qui interrompt l'écriture.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// L'utilisateur a annulé ; les fichiers déjà écrits le restent.
    #[error("XMP writing cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Où écrire : le dossier des copies des XMP d'autres logiciels, la date
/// de l'écriture.
#[derive(Debug, Clone)]
pub struct WriteSettings {
    /// Dossier des copies des XMP que PicLib n'a pas écrits, avant leur
    /// première modification.
    pub originals_dir: PathBuf,
    /// Date de l'écriture, en ISO 8601 (`xmp:MetadataDate`).
    pub metadata_date: String,
    /// Remplacer aussi les fichiers modifiés par un autre logiciel
    /// (« Écraser avec le catalogue »).
    pub overwrite_changed: bool,
}

/// Écrit le XMP des médias `ids`. `on_progress` reçoit le nombre de médias
/// traités et le total.
///
/// # Errors
///
/// Voir [`WriteError`].
pub fn write(
    catalog: &Mutex<Catalog>,
    ids: &[MediaId],
    settings: &WriteSettings,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<WriteReport, WriteError> {
    let total = ids.len() as u64;
    let mut report = WriteReport::default();
    let mut done = 0_u64;
    on_progress(0, total);
    for batch in ids.chunks(BATCH_SIZE) {
        if cancellation.is_cancelled() {
            return Err(WriteError::Cancelled);
        }
        let sources = xmp_sync::sources(lock(catalog).connection(), batch)?;
        let outcomes: Vec<(XmpSource, Outcome)> = sources
            .into_iter()
            .map(|source| {
                let outcome = write_one(&source, settings);
                (source, outcome)
            })
            .collect();
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        for (source, outcome) in &outcomes {
            match outcome {
                Outcome::Written(name, stamp) => {
                    xmp_sync::record_written(
                        &transaction,
                        source.id,
                        source.revision,
                        name,
                        *stamp,
                    )?;
                    report.written += 1;
                }
                Outcome::ChangedElsewhere => {
                    xmp_sync::record_changed_elsewhere(&transaction, source.id)?;
                    report.changed_elsewhere += 1;
                }
                Outcome::Failed(failure) => {
                    xmp_sync::record_error(&transaction, source.id, *failure)?;
                    report.failed += 1;
                }
            }
        }
        transaction.commit().map_err(CatalogError::from)?;
        drop(catalog);
        done += batch.len() as u64;
        on_progress(done, total);
    }
    Ok(report)
}

/// Résultat pour un média.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// Fichier écrit : son nom et son état.
    Written(String, FileStamp),
    ChangedElsewhere,
    Failed(WriteFailure),
}

fn write_one(source: &XmpSource, settings: &WriteSettings) -> Outcome {
    if source.missing || !source.path.exists() {
        return Outcome::Failed(WriteFailure::MissingPhoto);
    }
    if platform::is_on_camera_card(&source.path) {
        return Outcome::Failed(WriteFailure::CameraCard);
    }
    // Le fichier déjà écrit ou lu par PicLib reste le sien.
    let mut target = sidecar_target(source);
    if source
        .known
        .as_ref()
        .is_some_and(|known| Some(known.name.as_str()) != file_name(&target.path))
    {
        target = own_name_target(&source.path);
    }
    let mut current = match read_current(&target.path) {
        Ok(current) => current,
        Err(failure) => return Outcome::Failed(failure),
    };
    // Un `photo.xmp` qui décrit un autre fichier du même nom (« DNG ») n'est
    // pas le sien : le nom avec l'extension prend le relais.
    if let (Some(extension), Some((_, bytes))) = (&target.sidecar_for_extension, &current)
        && xmp::parse_packet(bytes)
            .ok()
            .and_then(|read| read.sidecar_for_extension)
            .is_some_and(|other| !other.eq_ignore_ascii_case(extension))
    {
        target = own_name_target(&source.path);
        current = match read_current(&target.path) {
            Ok(current) => current,
            Err(failure) => return Outcome::Failed(failure),
        };
    }
    let stamp = current.as_ref().map(|(stamp, _)| *stamp);
    let known = source.known.as_ref();
    let is_ours = known.is_some_and(|known| known.written && Some(known.stamp) == stamp);
    let changed_since_known =
        stamp.is_some() && known.is_some_and(|known| Some(known.stamp) != stamp);
    if changed_since_known && !settings.overwrite_changed {
        return Outcome::ChangedElsewhere;
    }
    let content = content(source, target.sidecar_for_extension.clone(), settings);
    let existing = current.as_ref().map(|(_, bytes)| bytes.as_slice());
    let packet = match xmp::write_packet(existing, &content) {
        Ok(packet) => packet,
        Err(XmpError::NotPreserved(reason)) => {
            tracing::error!(path = %target.path.display(), reason, "XMP merge refused");
            return Outcome::Failed(WriteFailure::NotPreserved);
        }
        Err(error) => {
            tracing::warn!(path = %target.path.display(), %error, "unreadable XMP left as is");
            return Outcome::Failed(WriteFailure::UnreadableSidecar);
        }
    };
    if stamp.is_some() && !is_ours {
        let backups = settings.originals_dir.join(&settings.metadata_date[..10]);
        if let Err(error) = files::keep_original_sidecar(&target.path, &backups) {
            tracing::warn!(path = %target.path.display(), %error, "cannot keep the original XMP");
            return Outcome::Failed(WriteFailure::WriteFailed);
        }
    }
    match files::write_sidecar(&target.path, &packet, stamp) {
        Ok(written) => Outcome::Written(
            file_name(&target.path).unwrap_or_default().to_owned(),
            written,
        ),
        Err(SidecarWriteError::ChangedMeanwhile) => Outcome::ChangedElsewhere,
        Err(error) => {
            tracing::warn!(path = %target.path.display(), %error, "cannot write the XMP");
            Outcome::Failed(WriteFailure::WriteFailed)
        }
    }
}

/// Nom d'un fichier, s'il est lisible.
fn file_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

/// État et contenu actuels du fichier annexe, s'il existe.
fn read_current(path: &Path) -> Result<Option<(FileStamp, Vec<u8>)>, WriteFailure> {
    let stamp = files::file_stamp(path).map_err(|_| WriteFailure::UnreadableSidecar)?;
    let Some(stamp) = stamp else {
        return Ok(None);
    };
    if stamp.size > xmp::MAX_SIDECAR_BYTES {
        return Err(WriteFailure::UnreadableSidecar);
    }
    let bytes = std::fs::read(path).map_err(|_| WriteFailure::UnreadableSidecar)?;
    Ok(Some((stamp, bytes)))
}

/// Bilan d'une recherche des XMP modifiés ailleurs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Fichiers modifiés par un autre logiciel depuis que PicLib les a
    /// écrits ou lus.
    pub changed: u64,
    /// Fichiers disparus : le média redevient « à enregistrer ».
    pub removed: u64,
}

/// Cherche, dans le dossier `folder` et ses sous-dossiers, les XMP connus
/// de PicLib qu'un autre logiciel a modifiés ou supprimés depuis : leur date ou leur taille a changé.
///
/// # Errors
///
/// Échoue si le catalogue ne peut pas être lu ou écrit.
pub fn check_folder(catalog: &Mutex<Catalog>, folder: i64) -> Result<CheckReport, CatalogError> {
    let known = xmp_sync::known_in_folder(lock(catalog).connection(), folder)?;
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    for (id, photo, sidecar) in known {
        let Some(dir) = photo.parent() else {
            continue;
        };
        match files::file_stamp(&dir.join(&sidecar.name)) {
            Ok(Some(stamp)) if stamp != sidecar.stamp => changed.push(id),
            // Disque débranché : rien à conclure.
            Ok(None) if dir.exists() => removed.push(id),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "cannot examine an XMP file");
            }
        }
    }
    if changed.is_empty() && removed.is_empty() {
        return Ok(CheckReport::default());
    }
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    for id in &changed {
        xmp_sync::record_changed_elsewhere(&transaction, *id)?;
    }
    for id in &removed {
        xmp_sync::forget_file(&transaction, *id)?;
    }
    transaction.commit()?;
    Ok(CheckReport {
        changed: changed.len() as u64,
        removed: removed.len() as u64,
    })
}

/// Retient les XMP que les médias `ids` viennent d'apporter (import) : un
/// autre logiciel les a écrits ; s'il les modifie ensuite, PicLib le
/// saura. À appeler dans la transaction de l'import.
///
/// # Errors
///
/// Échoue si le catalogue ne peut pas être lu ou écrit.
pub fn remember_read_sidecars(
    connection: &rusqlite::Connection,
    ids: &[MediaId],
) -> Result<(), CatalogError> {
    for source in xmp_sync::sources(connection, ids)? {
        let target = sidecar_target(&source);
        if let (Ok(Some(stamp)), Some(name)) =
            (files::file_stamp(&target.path), file_name(&target.path))
        {
            xmp_sync::record_read(connection, source.id, name, stamp)?;
        }
    }
    Ok(())
}

/// Fichier XMP d'un média tel que PicLib le lirait ou l'écrirait : celui
/// qu'il connaît déjà, sinon celui de [`sidecar_target`].
#[must_use]
pub fn sidecar_path(source: &XmpSource) -> PathBuf {
    match (&source.known, source.path.parent()) {
        (Some(known), Some(dir)) => dir.join(&known.name),
        _ => sidecar_target(source).path,
    }
}

/// Fichier annexe d'un média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarTarget {
    /// Chemin du fichier.
    pub path: PathBuf,
    /// Extension de la photo, écrite dans un `photo.xmp` partagé.
    pub sidecar_for_extension: Option<String>,
}

/// Fichier annexe que PicLib écrit pour `source` : `photo.xmp` pour un RAW
/// qui n'en partage pas le nom avec un autre RAW, sinon `photo.JPG.xmp`.
#[must_use]
pub fn sidecar_target(source: &XmpSource) -> SidecarTarget {
    let extension = source
        .path
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());
    match extension {
        Some(extension) if source.format.is_raw() && !source.shares_stem_with_raw => {
            SidecarTarget {
                path: source.path.with_extension("xmp"),
                sidecar_for_extension: Some(extension.to_uppercase()),
            }
        }
        _ => own_name_target(&source.path),
    }
}

fn own_name_target(photo: &Path) -> SidecarTarget {
    let mut name = photo.as_os_str().to_owned();
    name.push(".xmp");
    SidecarTarget {
        path: PathBuf::from(name),
        sidecar_for_extension: None,
    }
}

/// Ce que PicLib écrit pour `source`.
#[must_use]
pub fn content(
    source: &XmpSource,
    sidecar_for_extension: Option<String>,
    settings: &WriteSettings,
) -> XmpContent {
    let mut subjects: Vec<String> = Vec::new();
    for chain in &source.keywords {
        for (name, exported) in chain {
            if *exported && !subjects.contains(name) {
                subjects.push(name.clone());
            }
        }
    }
    XmpContent {
        rating: source.rating.stars(),
        flag: source.flag,
        color_label: source.color_label,
        fields: source.fields.clone(),
        gps: source.gps,
        orientation: (source.format.kind() == MediaKind::Photo)
            .then(|| displayed_orientation(source.orientation, source.rotation))
            .flatten(),
        capture_time: (source.capture_time != source.original_capture_time)
            .then(|| iso_date(&source.capture_time.0, source.capture_time.1)),
        keyword_paths: source
            .keywords
            .iter()
            .map(|chain| chain.iter().map(|(name, _)| name.clone()).collect())
            .collect(),
        subjects,
        sidecar_for_extension,
        metadata_date: settings.metadata_date.clone(),
    }
}

/// Orientation à l'affichage : celle du fichier (1 sans EXIF), tournée de la
/// rotation PicLib (sens des aiguilles d'une montre). Un miroir du fichier
/// est gardé.
#[must_use]
pub fn displayed_orientation(file: Option<Orientation>, rotation: Rotation) -> Option<Orientation> {
    // Valeur EXIF, miroir, angle.
    const ORIENTATIONS: [(u8, bool, u16); 8] = [
        (1, false, 0),
        (6, false, 90),
        (3, false, 180),
        (8, false, 270),
        (2, true, 0),
        (7, true, 90),
        (4, true, 180),
        (5, true, 270),
    ];
    let value = file.map_or(1, Orientation::get);
    let (_, mirrored, angle) = ORIENTATIONS
        .iter()
        .copied()
        .find(|(exif, _, _)| *exif == value)?;
    let turned = (angle + rotation.degrees()) % 360;
    ORIENTATIONS
        .iter()
        .find(|(_, other_mirrored, other_angle)| {
            *other_mirrored == mirrored && *other_angle == turned
        })
        .and_then(|(exif, _, _)| Orientation::new(*exif))
}

/// Date ISO 8601 avec son décalage UTC, s'il est connu.
fn iso_date(local: &str, offset_seconds: Option<i64>) -> String {
    match offset_seconds {
        Some(seconds) => {
            let sign = if seconds < 0 { '-' } else { '+' };
            let minutes = seconds.abs() / 60;
            format!("{local}{sign}{:02}:{:02}", minutes / 60, minutes % 60)
        }
        None => local.to_owned(),
    }
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
