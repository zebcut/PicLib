//! Suivi des vignettes au catalogue.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{media_format_from_text, text_to_path};
use crate::domain::{MediaFormat, MediaId, Timestamp};

/// Média qui attend sa vignette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingThumbnail {
    /// Média concerné.
    pub media_id: MediaId,
    /// Chemin complet du fichier d'origine.
    pub path: PathBuf,
    /// Format du fichier.
    pub format: MediaFormat,
    /// Date de modification du fichier connue du catalogue.
    pub file_modified_at: Timestamp,
}

/// État de la vignette d'un média.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailStatus {
    /// Pas encore fabriquée.
    Pending,
    /// Prête, dans le cache.
    Ready,
    /// Impossible à fabriquer (format inconnu du système, fichier abîmé,
    /// conteneur vidéo non pris en charge comme MKV ou AVI).
    Failed,
}

/// Vignette fabriquée, à enregistrer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyThumbnail {
    /// Largeur de la vignette, en pixels.
    pub width: u32,
    /// Hauteur de la vignette, en pixels.
    pub height: u32,
    /// Vrai si l'aperçu intégré au fichier a été utilisé.
    pub from_embedded_preview: bool,
}

/// Photos et vidéos qui attendent leur vignette, par ordre de date de prise
/// de vue décroissante : les plus récentes, les plus souvent regardées, sont
/// prêtes en premier.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn pending(connection: &Connection) -> Result<Vec<PendingThumbnail>, CatalogError> {
    let mut statement = connection.prepare(
        "SELECT media.id, folders.path, media.file_name, media.format, media.file_modified_at
         FROM media
         JOIN folders ON folders.id = media.folder_id
         LEFT JOIN thumbnails ON thumbnails.media_id = media.id
         WHERE thumbnails.media_id IS NULL
         ORDER BY media.capture_time DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    rows.map(|row| {
        let (id, folder, file_name, format, modified) = row?;
        Ok(PendingThumbnail {
            media_id: MediaId::new(id),
            path: text_to_path(folder).join(file_name),
            format: media_format_from_text(&format)?,
            file_modified_at: Timestamp::from_unix_millis(modified),
        })
    })
    .collect()
}

/// Enregistre une vignette prête.
///
/// # Errors
///
/// Échoue si le média n'existe pas ou si la base est illisible.
pub fn record_ready(
    connection: &Connection,
    pending: &PendingThumbnail,
    thumbnail: &ReadyThumbnail,
    generated_at: Timestamp,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "INSERT OR REPLACE INTO thumbnails (media_id, status, width, height, source, generated_at, file_modified_at)
         VALUES (?1, 'ready', ?2, ?3, ?4, ?5, ?6)",
    )?;
    statement.execute(params![
        pending.media_id.get(),
        thumbnail.width,
        thumbnail.height,
        if thumbnail.from_embedded_preview {
            "embedded"
        } else {
            "decoded"
        },
        generated_at.unix_millis(),
        pending.file_modified_at.unix_millis(),
    ])?;
    Ok(())
}

/// Enregistre l'échec de fabrication d'une vignette, pour ne pas la retenter
/// à chaque démarrage.
///
/// # Errors
///
/// Échoue si le média n'existe pas ou si la base est illisible.
pub fn record_failed(
    connection: &Connection,
    pending: &PendingThumbnail,
    attempted_at: Timestamp,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "INSERT OR REPLACE INTO thumbnails (media_id, status, generated_at, file_modified_at)
         VALUES (?1, 'failed', ?2, ?3)",
    )?;
    statement.execute(params![
        pending.media_id.get(),
        attempted_at.unix_millis(),
        pending.file_modified_at.unix_millis(),
    ])?;
    Ok(())
}

/// État de la vignette d'un média.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn status(connection: &Connection, media_id: MediaId) -> Result<ThumbnailStatus, CatalogError> {
    let status: Option<String> = connection
        .query_row(
            "SELECT status FROM thumbnails WHERE media_id = ?1",
            [media_id.get()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match status.as_deref() {
        None => ThumbnailStatus::Pending,
        Some("ready") => ThumbnailStatus::Ready,
        Some(_) => ThumbnailStatus::Failed,
    })
}

/// Complète les dimensions d'un média quand elles manquent (RAW dont l'EXIF
/// ne les donnait pas de façon fiable) ; ne remplace jamais des dimensions
/// déjà connues.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn fill_missing_dimensions(
    connection: &Connection,
    media_id: MediaId,
    width: u32,
    height: u32,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "UPDATE media SET width = ?1, height = ?2 WHERE id = ?3 AND (width IS NULL OR height IS NULL)",
    )?;
    statement.execute(params![width, height, media_id.get()])?;
    Ok(())
}
