//! Retouche dans un éditeur externe : photos
//! envoyées à un éditeur et liens « version traitée de ».

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::domain::{ImportId, MediaId, Timestamp};

/// Lien entre une version traitée et son original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditedVersion {
    /// Version traitée.
    pub media_id: MediaId,
    /// Original.
    pub original_id: MediaId,
    /// Éditeur qui l'a produite ; `None` : inconnu.
    pub editor: Option<String>,
    /// Photo envoyée à l'éditeur (le RAW, la copie `-Modifié`) ; `None` :
    /// l'original.
    pub working_id: Option<MediaId>,
    /// Empreinte d'une copie `-Modifié` à sa création.
    pub copy_digest: Option<String>,
}

/// Enregistre le lien d'une version (le remplace s'il existait).
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn insert(connection: &Connection, version: &EditedVersion) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT OR REPLACE INTO edited_versions
                 (media_id, original_id, editor, working_id, copy_digest)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![
            version.media_id.get(),
            version.original_id.get(),
            version.editor,
            version.working_id.map(MediaId::get),
            version.copy_digest,
        ])?;
    Ok(())
}

/// Lien de la version `media_id` ; `None` si ce n'est pas une version.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn get(
    connection: &Connection,
    media_id: MediaId,
) -> Result<Option<EditedVersion>, CatalogError> {
    Ok(connection
        .prepare_cached(
            "SELECT original_id, editor, working_id, copy_digest
             FROM edited_versions WHERE media_id = ?1",
        )?
        .query_row([media_id.get()], |row| {
            Ok(EditedVersion {
                media_id,
                original_id: MediaId::new(row.get(0)?),
                editor: row.get(1)?,
                working_id: row.get::<_, Option<i64>>(2)?.map(MediaId::new),
                copy_digest: row.get(3)?,
            })
        })
        .optional()?)
}

/// Note l'envoi de la photo `media_id` à l'éditeur `editor` : le premier
/// envoi ouvre le suivi, les suivants le prolongent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn record_sent(
    connection: &Connection,
    media_id: MediaId,
    editor: &str,
    now: Timestamp,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO editor_sessions (media_id, editor, sent_at, active_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (media_id, editor) DO UPDATE SET active_at = excluded.active_at",
        )?
        .execute(params![media_id.get(), editor, now.unix_millis()])?;
    Ok(())
}

/// Note que l'import `import_id` n'a ajouté que des versions traitées : il
/// ne devient pas « Import précédent ».
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn mark_import(connection: &Connection, import_id: ImportId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE imports SET edited_versions = 1 WHERE id = ?1")?
        .execute([import_id.get()])?;
    Ok(())
}

/// Donne à la version `to` les métadonnées de son original `from` (toutes) : note, marqueur, couleur, champs descriptifs et lieu,
/// position GPS, mots-clés ; et sa date de prise de vue si elle a été
/// modifiée dans PicLib, ou si le fichier de la version n'en donne pas
/// (certains éditeurs, DxO PhotoLab compris, ne la recopient pas : la
/// version aurait sinon la date de l'export et s'éloignerait de son
/// original dans la grille). Sans date propre, la version prend aussi la
/// date d'origine de l'original : elle n'apparaît pas comme modifiée. La
/// rotation n'est pas reprise : l'éditeur l'a appliquée à l'image.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn copy_metadata(
    connection: &Connection,
    from: MediaId,
    to: MediaId,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE media SET
                 flag = original.flag, rating = original.rating,
                 color_label = original.color_label,
                 title = original.title, caption = original.caption,
                 creator = original.creator, copyright = original.copyright,
                 country = original.country, region = original.region,
                 city = original.city, location = original.location,
                 gps_latitude = original.gps_latitude,
                 gps_longitude = original.gps_longitude,
                 gps_altitude = original.gps_altitude
             FROM (SELECT * FROM media WHERE id = ?1) AS original
             WHERE media.id = ?2",
        )?
        .execute(params![from.get(), to.get()])?;
    // Sans date dans son fichier : toutes les dates de l'original.
    connection
        .prepare_cached(
            "UPDATE media SET
                 capture_time = original.capture_time,
                 capture_offset_seconds = original.capture_offset_seconds,
                 capture_time_source = original.capture_time_source,
                 original_capture_time = original.original_capture_time,
                 original_capture_offset_seconds = original.original_capture_offset_seconds
             FROM (SELECT * FROM media WHERE id = ?1) AS original
             WHERE media.id = ?2 AND media.capture_time_source = 'file'",
        )?
        .execute(params![from.get(), to.get()])?;
    connection
        .prepare_cached(
            "UPDATE media SET
                 capture_time = original.capture_time,
                 capture_offset_seconds = original.capture_offset_seconds,
                 capture_time_source = original.capture_time_source
             FROM (SELECT * FROM media WHERE id = ?1) AS original
             WHERE media.id = ?2
               AND (original.capture_time IS NOT original.original_capture_time
                    OR original.capture_offset_seconds
                       IS NOT original.original_capture_offset_seconds)",
        )?
        .execute(params![from.get(), to.get()])?;
    connection
        .prepare_cached(
            "INSERT OR IGNORE INTO media_keywords (media_id, keyword_id)
             SELECT ?2, keyword_id FROM media_keywords WHERE media_id = ?1",
        )?
        .execute(params![from.get(), to.get()])?;
    Ok(())
}
