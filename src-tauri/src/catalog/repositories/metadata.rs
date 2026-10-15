//! Métadonnées descriptives (titre, légende, auteur…) d'un ensemble de
//! médias, et informations affichées par le panneau Métadonnées.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders, size_from_sql, text_to_path};
use crate::domain::{GpsPosition, MediaDetails, MediaId, MetadataField};

/// Colonne d'un champ dans la table `media`.
pub(super) fn column(field: MetadataField) -> &'static str {
    match field {
        MetadataField::Title => "title",
        MetadataField::Caption => "caption",
        MetadataField::Creator => "creator",
        MetadataField::Copyright => "copyright",
        MetadataField::Country => "country",
        MetadataField::Region => "region",
        MetadataField::City => "city",
        MetadataField::Location => "location",
    }
}

/// Ce que montrent des médias sélectionnés pour un champ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSummary {
    /// Tous ont la même valeur (`None` : tous vides).
    Same(Option<String>),
    /// Les valeurs diffèrent (« < valeurs mixtes > »).
    Mixed,
}

/// Valeur actuelle d'un champ pour chaque média existant parmi `ids` (les
/// identifiants inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn read(
    connection: &Connection,
    field: MetadataField,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, Option<String>)>, CatalogError> {
    let mut values = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id, {} FROM media WHERE id IN ({})",
            column(field),
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = params_from_iter(chunk.iter().map(|id| id.get()));
        let rows = statement.query_map(parameters, |row| {
            Ok((MediaId::new(row.get(0)?), row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            values.push(row?);
        }
    }
    Ok(values)
}

/// Donne la valeur `value` (`None` : vide) au champ `field` des médias
/// `ids` ; renvoie le nombre de médias modifiés. À appeler dans une
/// transaction pour un ensemble cohérent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn write(
    connection: &Connection,
    field: MetadataField,
    value: Option<&str>,
    ids: &[MediaId],
) -> Result<usize, CatalogError> {
    let sql_value = value.map_or(Value::Null, |text| Value::Text(text.to_owned()));
    let mut changed = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "UPDATE media SET {} = ?1 WHERE id IN ({})",
            column(field),
            placeholders(2, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = std::iter::once(sql_value.clone())
            .chain(chunk.iter().map(|id| Value::Integer(id.get())));
        changed += statement.execute(params_from_iter(parameters))?;
    }
    Ok(changed)
}

/// Pour chaque champ, dans l'ordre de [`MetadataField::ALL`] : la valeur
/// commune des médias `ids`, ou « valeurs mixtes ».
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn summary(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(MetadataField, FieldSummary)>, CatalogError> {
    let mut tallies = [(); MetadataField::ALL.len()].map(|()| Tally::default());
    // Plus petite et plus grande valeur, et nombre de valeurs non vides, de
    // chaque champ : une seule requête par paquet d'identifiants suffit pour
    // savoir si les valeurs diffèrent, même pour 50 000 médias.
    let aggregates = MetadataField::ALL
        .iter()
        .map(|&field| {
            let column = column(field);
            format!("min({column}), max({column}), count({column})")
        })
        .collect::<Vec<_>>()
        .join(", ");
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT count(*), {aggregates} FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = params_from_iter(chunk.iter().map(|id| id.get()));
        statement.query_row(parameters, |row| {
            let total: i64 = row.get(0)?;
            for (index, tally) in tallies.iter_mut().enumerate() {
                let first = 1 + 3 * index;
                tally.add(
                    total,
                    row.get(first)?,
                    row.get(first + 1)?,
                    row.get(first + 2)?,
                );
            }
            Ok(())
        })?;
    }
    Ok(MetadataField::ALL
        .into_iter()
        .zip(tallies.into_iter().map(Tally::summary))
        .collect())
}

/// Décompte des valeurs d'un champ sur plusieurs paquets de médias.
#[derive(Debug, Default)]
struct Tally {
    total: i64,
    non_empty: i64,
    min: Option<String>,
    max: Option<String>,
}

impl Tally {
    fn add(&mut self, total: i64, min: Option<String>, max: Option<String>, non_empty: i64) {
        self.total += total;
        self.non_empty += non_empty;
        if let Some(min) = min
            && self.min.as_ref().is_none_or(|current| min < *current)
        {
            self.min = Some(min);
        }
        if let Some(max) = max
            && self.max.as_ref().is_none_or(|current| max > *current)
        {
            self.max = Some(max);
        }
    }

    fn summary(self) -> FieldSummary {
        if self.non_empty == 0 {
            FieldSummary::Same(None)
        } else if self.non_empty < self.total || self.min != self.max {
            // Certains vides et d'autres non, ou des textes différents.
            FieldSummary::Mixed
        } else {
            FieldSummary::Same(self.min)
        }
    }
}

/// Informations en lecture seule d'un média, pour le panneau Métadonnées.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    /// Chemin complet du fichier.
    pub path: PathBuf,
    /// Taille du fichier en octets.
    pub file_size: u64,
    /// Date de prise de vue locale (« 2024-07-14T18:30:05 »).
    pub capture_time: String,
    /// Date de prise de vue lue à l'import (différente si elle a été
    /// modifiée).
    pub original_capture_time: String,
    /// Informations techniques (dimensions, EXIF, GPS…).
    pub details: MediaDetails,
}

/// Informations en lecture seule du média `id`, s'il existe.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn info(connection: &Connection, id: MediaId) -> Result<Option<MediaInfo>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT folders.path, media.file_name, media.file_size,
                media.capture_time, media.width, media.height, media.camera_make,
                media.camera_model, media.lens, media.focal_length, media.aperture,
                media.exposure_time, media.iso, media.flash_fired, media.gps_latitude,
                media.gps_longitude, media.gps_altitude, media.duration_ms,
                media.original_capture_time
         FROM media JOIN folders ON folders.id = media.folder_id
         WHERE media.id = ?1",
    )?;
    let row = statement
        .query_row([id.get()], |row| {
            let latitude: Option<f64> = row.get(14)?;
            let longitude: Option<f64> = row.get(15)?;
            let altitude: Option<f64> = row.get(16)?;
            let details = MediaDetails {
                width: row.get(4)?,
                height: row.get(5)?,
                orientation: None,
                camera_make: row.get(6)?,
                camera_model: row.get(7)?,
                lens: row.get(8)?,
                focal_length_mm: row.get(9)?,
                aperture: row.get(10)?,
                exposure_time_s: row.get(11)?,
                iso: row.get(12)?,
                flash_fired: row.get(13)?,
                gps: latitude
                    .zip(longitude)
                    .map(|(latitude, longitude)| GpsPosition {
                        latitude,
                        longitude,
                        altitude,
                    }),
                duration_ms: None,
            };
            Ok(RawInfo {
                folder: row.get(0)?,
                file_name: row.get(1)?,
                file_size: row.get(2)?,
                capture_time: row.get(3)?,
                duration_ms: row.get(17)?,
                original_capture_time: row.get(18)?,
                details,
            })
        })
        .optional()?;
    row.map(|raw| {
        Ok(MediaInfo {
            path: text_to_path(raw.folder).join(raw.file_name),
            file_size: size_from_sql(raw.file_size)?,
            capture_time: raw.capture_time,
            original_capture_time: raw.original_capture_time,
            details: MediaDetails {
                duration_ms: raw.duration_ms.map(size_from_sql).transpose()?,
                ..raw.details
            },
        })
    })
    .transpose()
}

/// Ligne lue telle quelle, avant conversion des valeurs.
struct RawInfo {
    folder: String,
    file_name: String,
    file_size: i64,
    capture_time: String,
    original_capture_time: String,
    duration_ms: Option<i64>,
    details: MediaDetails,
}
