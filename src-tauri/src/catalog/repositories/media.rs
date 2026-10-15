//! Photos et vidéos du catalogue.

use std::path::PathBuf;

use jiff::tz::Offset;
use rusqlite::{Connection, OptionalExtension, Row, named_params, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{
    IDS_PER_QUERY, capture_source_from_text, capture_source_to_text, media_format_from_text,
    media_format_to_text, media_kind_to_text, placeholders, size_from_sql, size_to_sql,
    text_to_path,
};
use crate::domain::{
    CaptureTime, FolderId, GpsPosition, ImportId, Media, MediaDetails, MediaId, MediaKind,
    NewMedia, Orientation, Timestamp,
};

const COLUMNS: &str = "id, folder_id, file_name, original_file_name, format, file_size, \
    file_modified_at, capture_time, capture_offset_seconds, capture_time_source, width, height, \
    orientation, camera_make, camera_model, lens, focal_length, aperture, exposure_time, iso, \
    flash_fired, gps_latitude, gps_longitude, gps_altitude, duration_ms, import_id, content_hash";

/// Nombre de photos et de vidéos du catalogue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaCounts {
    /// Photos.
    pub photos: u64,
    /// Vidéos.
    pub videos: u64,
}

/// Ajoute un média et renvoie son identifiant.
///
/// # Errors
///
/// Échoue si le fichier est déjà au catalogue dans ce dossier, si le dossier ou
/// l'import n'existe pas, ou si la base est illisible.
pub fn insert(connection: &Connection, media: &NewMedia) -> Result<MediaId, CatalogError> {
    let details = &media.details;
    let gps = details.gps;
    // Requête préparée une fois et réutilisée : l'import en ajoute des milliers.
    // L'identifiant n'est jamais celui d'un média retiré : il dépasse le plus
    // grand jamais attribué (table `id_watermarks`, migration n° 6).
    let mut statement = connection.prepare_cached(
        "INSERT INTO media (id, folder_id, file_name, kind, file_size, file_modified_at, import_id,
            original_file_name, name_key, format, capture_time, capture_offset_seconds,
            original_capture_time, original_capture_offset_seconds,
            capture_time_source, width, height, orientation, camera_make, camera_model, lens,
            focal_length, aperture, exposure_time, iso, flash_fired, gps_latitude, gps_longitude,
            gps_altitude, duration_ms, content_hash)
         VALUES ((SELECT max(coalesce((SELECT max(id) FROM media), 0), last_id) + 1
                  FROM id_watermarks WHERE name = 'media'),
            :folder_id, :file_name, :kind, :file_size, :file_modified_at, :import_id,
            :original_file_name, :name_key, :format, :capture_time, :capture_offset_seconds,
            :capture_time, :capture_offset_seconds,
            :capture_time_source, :width, :height, :orientation, :camera_make, :camera_model, :lens,
            :focal_length, :aperture, :exposure_time, :iso, :flash_fired, :gps_latitude,
            :gps_longitude, :gps_altitude, :duration_ms, :content_hash)",
    )?;
    statement.execute(named_params! {
        ":folder_id": media.folder_id.get(),
        ":file_name": media.file_name,
        ":kind": media_kind_to_text(media.kind()),
        ":file_size": size_to_sql(media.file_size)?,
        ":file_modified_at": media.file_modified_at.unix_millis(),
        ":import_id": media.import_id.get(),
        ":original_file_name": media.original_file_name,
        ":name_key": media.name_key(),
        ":format": media_format_to_text(media.format),
        ":capture_time": media.capture_time.storage_text(),
        ":capture_offset_seconds": media.capture_time.offset.map(Offset::seconds),
        ":capture_time_source": capture_source_to_text(media.capture_time.source),
        ":width": details.width,
        ":height": details.height,
        ":orientation": details.orientation.map(Orientation::get),
        ":camera_make": details.camera_make,
        ":camera_model": details.camera_model,
        ":lens": details.lens,
        ":focal_length": details.focal_length_mm,
        ":aperture": details.aperture,
        ":exposure_time": details.exposure_time_s,
        ":iso": details.iso,
        ":flash_fired": details.flash_fired,
        ":gps_latitude": gps.map(|position| position.latitude),
        ":gps_longitude": gps.map(|position| position.longitude),
        ":gps_altitude": gps.and_then(|position| position.altitude),
        ":duration_ms": details.duration_ms.map(size_to_sql).transpose()?,
        ":content_hash": media.content_hash,
    })?;
    Ok(MediaId::new(connection.last_insert_rowid()))
}

/// Indique si un fichier de ce nom est déjà au catalogue dans ce dossier.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn exists_in_folder(
    connection: &Connection,
    folder_id: FolderId,
    file_name: &str,
) -> Result<bool, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT EXISTS (SELECT 1 FROM media WHERE folder_id = ?1 AND file_name = ?2)",
    )?;
    Ok(statement.query_row(params![folder_id.get(), file_name], |row| row.get(0))?)
}

/// Cherche un doublon : même nom d'origine (clé normalisée), même taille et
/// même date de prise de vue **d'origine** (celle lue à l'import, jamais modifiée).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_duplicate(
    connection: &Connection,
    name_key: &str,
    file_size: u64,
    capture_time: &CaptureTime,
) -> Result<Option<MediaId>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT id FROM media
         WHERE name_key = ?1 AND file_size = ?2 AND original_capture_time = ?3 LIMIT 1",
    )?;
    let id = statement
        .query_row(
            params![
                name_key,
                size_to_sql(file_size)?,
                capture_time.storage_text()
            ],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id.map(MediaId::new))
}

/// Cherche un média dont le contenu a l'empreinte `content_hash` (import par copie : même photo sous un autre nom).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_by_content_hash(
    connection: &Connection,
    content_hash: &str,
) -> Result<Option<MediaId>, CatalogError> {
    let id = connection
        .prepare_cached("SELECT id FROM media WHERE content_hash = ?1 LIMIT 1")?
        .query_row([content_hash], |row| row.get(0))
        .optional()?;
    Ok(id.map(MediaId::new))
}

/// Cherche un média par son dossier et son nom de fichier exact.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn find_in_folder(
    connection: &Connection,
    folder_id: FolderId,
    file_name: &str,
) -> Result<Option<Media>, CatalogError> {
    let sql = format!("SELECT {COLUMNS} FROM media WHERE folder_id = ?1 AND file_name = ?2");
    connection
        .query_row(&sql, params![folder_id.get(), file_name], MediaRow::read)
        .optional()?
        .map(Media::try_from)
        .transpose()
}

/// Média dont le fichier est `path`, s'il est au catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_in_folder_path(
    connection: &Connection,
    path: &std::path::Path,
) -> Result<Option<MediaId>, CatalogError> {
    let (Some(folder), Some(name)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Ok(None);
    };
    let Some(folder) = super::folders::find_by_path(connection, folder)? else {
        return Ok(None);
    };
    let id = connection
        .prepare_cached("SELECT id FROM media WHERE folder_id = ?1 AND file_name = ?2")?
        .query_row(params![folder.id.get(), name], |row| row.get(0))
        .optional()?;
    Ok(id.map(MediaId::new))
}

/// Médias d'un dossier, triés par nom de fichier.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn list_in_folder(
    connection: &Connection,
    folder_id: FolderId,
) -> Result<Vec<Media>, CatalogError> {
    let sql = format!("SELECT {COLUMNS} FROM media WHERE folder_id = ?1 ORDER BY file_name");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([folder_id.get()], MediaRow::read)?;
    rows.map(|row| Media::try_from(row?)).collect()
}

/// Médias existants parmi `ids`, avec le chemin complet de leur fichier
/// (les identifiants inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn read_with_paths(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(Media, PathBuf)>, CatalogError> {
    let columns = COLUMNS
        .split(", ")
        .map(|column| format!("media.{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut found = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT {columns}, folders.path AS folder_path
             FROM media JOIN folders ON folders.id = media.folder_id
             WHERE media.id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((MediaRow::read(row)?, row.get::<_, String>("folder_path")?))
            })?;
        for row in rows {
            let (row, folder) = row?;
            let media = Media::try_from(row)?;
            let path = text_to_path(folder).join(&media.data.file_name);
            found.push((media, path));
        }
    }
    Ok(found)
}

/// Range le média `id` dans le dossier `folder` sous le nom `file_name`
/// (reprise d'un déplacement interrompu).
///
/// # Errors
///
/// Échoue si la base est illisible, ou si le nom est déjà pris dans ce
/// dossier.
pub fn set_place(
    connection: &Connection,
    id: MediaId,
    folder: FolderId,
    file_name: &str,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE media SET folder_id = ?2, file_name = ?3 WHERE id = ?1")?
        .execute(params![id.get(), folder.get(), file_name])?;
    Ok(())
}

/// Nombre total de médias dans le catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count(connection: &Connection) -> Result<u64, CatalogError> {
    let counts = count_by_kind(connection)?;
    Ok(counts.photos + counts.videos)
}

/// Nombre de photos et de vidéos du catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count_by_kind(connection: &Connection) -> Result<MediaCounts, CatalogError> {
    let mut statement = connection.prepare("SELECT kind, count(*) FROM media GROUP BY kind")?;
    let mut counts = MediaCounts::default();
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (kind, count) = row?;
        let count = size_from_sql(count)?;
        match crate::catalog::sql::media_kind_from_text(&kind)? {
            MediaKind::Photo => counts.photos = count,
            MediaKind::Video => counts.videos = count,
        }
    }
    Ok(counts)
}

/// Ligne brute lue en base, avant contrôle des valeurs.
struct MediaRow {
    id: i64,
    folder_id: i64,
    file_name: String,
    original_file_name: String,
    format: String,
    file_size: i64,
    file_modified_at: i64,
    capture_time: String,
    capture_offset_seconds: Option<i32>,
    capture_time_source: String,
    width: Option<u32>,
    height: Option<u32>,
    orientation: Option<u8>,
    camera_make: Option<String>,
    camera_model: Option<String>,
    lens: Option<String>,
    focal_length: Option<f64>,
    aperture: Option<f64>,
    exposure_time: Option<f64>,
    iso: Option<u32>,
    flash_fired: Option<bool>,
    gps_latitude: Option<f64>,
    gps_longitude: Option<f64>,
    gps_altitude: Option<f64>,
    duration_ms: Option<i64>,
    import_id: i64,
    content_hash: Option<String>,
}

impl MediaRow {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            folder_id: row.get("folder_id")?,
            file_name: row.get("file_name")?,
            original_file_name: row.get("original_file_name")?,
            format: row.get("format")?,
            file_size: row.get("file_size")?,
            file_modified_at: row.get("file_modified_at")?,
            capture_time: row.get("capture_time")?,
            capture_offset_seconds: row.get("capture_offset_seconds")?,
            capture_time_source: row.get("capture_time_source")?,
            width: row.get("width")?,
            height: row.get("height")?,
            orientation: row.get("orientation")?,
            camera_make: row.get("camera_make")?,
            camera_model: row.get("camera_model")?,
            lens: row.get("lens")?,
            focal_length: row.get("focal_length")?,
            aperture: row.get("aperture")?,
            exposure_time: row.get("exposure_time")?,
            iso: row.get("iso")?,
            flash_fired: row.get("flash_fired")?,
            gps_latitude: row.get("gps_latitude")?,
            gps_longitude: row.get("gps_longitude")?,
            gps_altitude: row.get("gps_altitude")?,
            duration_ms: row.get("duration_ms")?,
            import_id: row.get("import_id")?,
            content_hash: row.get("content_hash")?,
        })
    }
}

impl TryFrom<MediaRow> for Media {
    type Error = CatalogError;

    fn try_from(row: MediaRow) -> Result<Self, Self::Error> {
        let local = CaptureTime::parse_storage_text(&row.capture_time).map_err(|_| {
            CatalogError::InvalidValue {
                column: "capture_time",
                value: row.capture_time.clone(),
            }
        })?;
        let offset = row
            .capture_offset_seconds
            .map(|seconds| {
                Offset::from_seconds(seconds).map_err(|_| CatalogError::InvalidValue {
                    column: "capture_offset_seconds",
                    value: seconds.to_string(),
                })
            })
            .transpose()?;
        let gps = match (row.gps_latitude, row.gps_longitude) {
            (Some(latitude), Some(longitude)) => Some(GpsPosition {
                latitude,
                longitude,
                altitude: row.gps_altitude,
            }),
            _ => None,
        };

        Ok(Self {
            id: MediaId::new(row.id),
            data: NewMedia {
                folder_id: FolderId::new(row.folder_id),
                file_name: row.file_name,
                original_file_name: row.original_file_name,
                format: media_format_from_text(&row.format)?,
                file_size: size_from_sql(row.file_size)?,
                file_modified_at: Timestamp::from_unix_millis(row.file_modified_at),
                capture_time: CaptureTime {
                    local,
                    offset,
                    source: capture_source_from_text(&row.capture_time_source)?,
                },
                details: MediaDetails {
                    width: row.width,
                    height: row.height,
                    orientation: row.orientation.and_then(Orientation::new),
                    camera_make: row.camera_make,
                    camera_model: row.camera_model,
                    lens: row.lens,
                    focal_length_mm: row.focal_length,
                    aperture: row.aperture,
                    exposure_time_s: row.exposure_time,
                    iso: row.iso,
                    flash_fired: row.flash_fired,
                    gps,
                    duration_ms: row.duration_ms.map(size_from_sql).transpose()?,
                },
                import_id: ImportId::new(row.import_id),
                content_hash: row.content_hash,
            },
        })
    }
}
