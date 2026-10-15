//! Conversions entre les types métier et les valeurs stockées en base.

use std::path::{Path, PathBuf};

use super::CatalogError;
use crate::domain::{
    CaptureTimeSource, ColorLabel, Flag, ImportMode, ImportStatus, MediaFormat, MediaKind, Rating,
    Rotation,
};

/// Texte d'un chemin, tel qu'enregistré en base.
pub(super) fn path_to_text(path: &Path) -> Result<&str, CatalogError> {
    path.to_str()
        .ok_or_else(|| CatalogError::NonUtf8Path(path.to_path_buf()))
}

/// Chemin enregistré en base.
pub(super) fn text_to_path(text: String) -> PathBuf {
    PathBuf::from(text)
}

pub(super) fn media_kind_to_text(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Photo => "photo",
        MediaKind::Video => "video",
    }
}

pub(crate) fn media_kind_from_text(text: &str) -> Result<MediaKind, CatalogError> {
    match text {
        "photo" => Ok(MediaKind::Photo),
        "video" => Ok(MediaKind::Video),
        other => Err(invalid("kind", other)),
    }
}

pub(super) fn import_mode_to_text(mode: ImportMode) -> &'static str {
    match mode {
        ImportMode::Reference => "reference",
        ImportMode::Copy => "copy",
    }
}

pub(super) fn import_mode_from_text(text: &str) -> Result<ImportMode, CatalogError> {
    match text {
        "reference" => Ok(ImportMode::Reference),
        "copy" => Ok(ImportMode::Copy),
        other => Err(invalid("mode", other)),
    }
}

pub(super) fn media_format_to_text(format: MediaFormat) -> &'static str {
    match format {
        MediaFormat::Jpeg => "jpeg",
        MediaFormat::Png => "png",
        MediaFormat::Heic => "heic",
        MediaFormat::Tiff => "tiff",
        MediaFormat::Psd => "psd",
        MediaFormat::Cr2 => "cr2",
        MediaFormat::Cr3 => "cr3",
        MediaFormat::Nef => "nef",
        MediaFormat::Arw => "arw",
        MediaFormat::Raf => "raf",
        MediaFormat::Dng => "dng",
        MediaFormat::Mp4 => "mp4",
        MediaFormat::Mov => "mov",
        MediaFormat::M4v => "m4v",
        MediaFormat::Avi => "avi",
        MediaFormat::Mts => "mts",
        MediaFormat::ThreeGp => "3gp",
        MediaFormat::Mkv => "mkv",
    }
}

pub(super) fn media_format_from_text(text: &str) -> Result<MediaFormat, CatalogError> {
    MediaFormat::from_extension(text).ok_or_else(|| invalid("format", text))
}

pub(super) fn capture_source_to_text(source: CaptureTimeSource) -> &'static str {
    match source {
        CaptureTimeSource::Exif => "exif",
        CaptureTimeSource::FileModified => "file",
    }
}

pub(super) fn capture_source_from_text(text: &str) -> Result<CaptureTimeSource, CatalogError> {
    match text {
        "exif" => Ok(CaptureTimeSource::Exif),
        "file" => Ok(CaptureTimeSource::FileModified),
        other => Err(invalid("capture_time_source", other)),
    }
}

pub(super) fn import_status_to_text(status: ImportStatus) -> &'static str {
    match status {
        ImportStatus::Running => "running",
        ImportStatus::Completed => "completed",
        ImportStatus::Cancelled => "cancelled",
        ImportStatus::Failed => "failed",
    }
}

pub(super) fn import_status_from_text(text: &str) -> Result<ImportStatus, CatalogError> {
    match text {
        "running" => Ok(ImportStatus::Running),
        "completed" => Ok(ImportStatus::Completed),
        "cancelled" => Ok(ImportStatus::Cancelled),
        "failed" => Ok(ImportStatus::Failed),
        other => Err(invalid("status", other)),
    }
}

/// SQLite ne stocke que des entiers signés : une taille au-delà de 8 Eio
/// (impossible en pratique) est refusée plutôt que tronquée.
pub(super) fn size_to_sql(size: u64) -> Result<i64, CatalogError> {
    i64::try_from(size).map_err(|_| invalid("file_size", &size.to_string()))
}

pub(super) fn size_from_sql(size: i64) -> Result<u64, CatalogError> {
    u64::try_from(size).map_err(|_| invalid("file_size", &size.to_string()))
}

pub(super) fn flag_to_text(flag: Flag) -> &'static str {
    match flag {
        Flag::Picked => "picked",
        Flag::Rejected => "rejected",
    }
}

pub(super) fn flag_from_text(text: &str) -> Result<Flag, CatalogError> {
    match text {
        "picked" => Ok(Flag::Picked),
        "rejected" => Ok(Flag::Rejected),
        other => Err(invalid("flag", other)),
    }
}

pub(super) fn color_label_to_text(color: ColorLabel) -> &'static str {
    match color {
        ColorLabel::Red => "red",
        ColorLabel::Yellow => "yellow",
        ColorLabel::Green => "green",
        ColorLabel::Blue => "blue",
        ColorLabel::Purple => "purple",
    }
}

pub(super) fn color_label_from_text(text: &str) -> Result<ColorLabel, CatalogError> {
    match text {
        "red" => Ok(ColorLabel::Red),
        "yellow" => Ok(ColorLabel::Yellow),
        "green" => Ok(ColorLabel::Green),
        "blue" => Ok(ColorLabel::Blue),
        "purple" => Ok(ColorLabel::Purple),
        other => Err(invalid("color_label", other)),
    }
}

pub(super) fn rating_from_sql(stars: i64) -> Result<Rating, CatalogError> {
    u8::try_from(stars)
        .ok()
        .and_then(Rating::new)
        .ok_or_else(|| invalid("rating", &stars.to_string()))
}

/// Nombre d'identifiants par requête : SQLite limite le nombre de paramètres
/// d'une requête ; au-delà, on découpe la liste.
pub(super) const IDS_PER_QUERY: usize = 500;

/// « ?2, ?3, ... » : `count` paramètres numérotés à partir de `first`.
pub(super) fn placeholders(first: usize, count: usize) -> String {
    (first..first + count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn rotation_from_sql(degrees: i64) -> Result<Rotation, CatalogError> {
    u16::try_from(degrees)
        .ok()
        .and_then(Rotation::from_degrees)
        .ok_or_else(|| invalid("rotation", &degrees.to_string()))
}

fn invalid(column: &'static str, value: &str) -> CatalogError {
    CatalogError::InvalidValue {
        column,
        value: value.to_owned(),
    }
}
