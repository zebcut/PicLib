//! Date de prise de vue modifiable, et date d'origine (migration 10).

use rusqlite::{Connection, params, params_from_iter};

use super::metadata::FieldSummary;
use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, capture_source_to_text, placeholders};
use crate::domain::{CaptureTimeSource, MediaId};

/// Date de prise de vue telle qu'enregistrée : texte de la date locale
/// (`2024-07-14T18:30:05`) et décalage par rapport à UTC, s'il est connu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCaptureTime {
    /// Date locale, au format de stockage.
    pub local: String,
    /// Décalage par rapport à UTC, en secondes.
    pub offset_seconds: Option<i32>,
}

/// Date actuelle et date d'origine d'un média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTimes {
    /// Date actuelle (éventuellement modifiée).
    pub current: StoredCaptureTime,
    /// Date lue à l'import, jamais modifiée.
    pub original: StoredCaptureTime,
}

/// Dates de chaque média existant parmi `ids` (les identifiants inconnus
/// sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn read(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, CaptureTimes)>, CatalogError> {
    let mut values = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id, capture_time, capture_offset_seconds,
                    original_capture_time, original_capture_offset_seconds
             FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows =
            statement.query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((
                    MediaId::new(row.get(0)?),
                    CaptureTimes {
                        current: StoredCaptureTime {
                            local: row.get(1)?,
                            offset_seconds: row.get(2)?,
                        },
                        original: StoredCaptureTime {
                            local: row.get(3)?,
                            offset_seconds: row.get(4)?,
                        },
                    },
                ))
            })?;
        for row in rows {
            values.push(row?);
        }
    }
    Ok(values)
}

/// Donne la date `time` au média `id` (la date d'origine ne change pas). À
/// appeler dans une transaction pour un ensemble cohérent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn write(
    connection: &Connection,
    id: MediaId,
    time: &StoredCaptureTime,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE media SET capture_time = ?1, capture_offset_seconds = ?2 WHERE id = ?3",
        )?
        .execute(params![time.local, time.offset_seconds, id.get()])?;
    Ok(())
}

/// Remplace la date actuelle **et** la date d'origine du média `id`, avec
/// leur origine : pour une date lue plus tard dans le fichier, quand l'import
/// n'avait trouvé que la date du fichier. La recherche des
/// doublons se fie ensuite à la nouvelle date d'origine. À appeler dans une
/// transaction.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn write_with_original(
    connection: &Connection,
    id: MediaId,
    times: &CaptureTimes,
    source: CaptureTimeSource,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE media SET capture_time = ?1, capture_offset_seconds = ?2,
                original_capture_time = ?3, original_capture_offset_seconds = ?4,
                capture_time_source = ?5
             WHERE id = ?6",
        )?
        .execute(params![
            times.current.local,
            times.current.offset_seconds,
            times.original.local,
            times.original.offset_seconds,
            capture_source_to_text(source),
            id.get()
        ])?;
    Ok(())
}

/// Date des médias `ids` pour le panneau Métadonnées : la date commune, ou
/// « valeurs mixtes », et le nombre de médias dont la date n'est plus celle
/// d'origine.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn summary(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<(FieldSummary, u64), CatalogError> {
    let mut earliest: Option<String> = None;
    let mut latest: Option<String> = None;
    let mut modified: i64 = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT min(capture_time), max(capture_time),
                    coalesce(sum(capture_time IS NOT original_capture_time
                                 OR capture_offset_seconds IS NOT original_capture_offset_seconds),
                             0)
             FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let (min, max, count): (Option<String>, Option<String>, i64) = connection
            .prepare_cached(&sql)?
            .query_row(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        if let Some(min) = min
            && earliest.as_ref().is_none_or(|current| min < *current)
        {
            earliest = Some(min);
        }
        if let Some(max) = max
            && latest.as_ref().is_none_or(|current| max > *current)
        {
            latest = Some(max);
        }
        modified += count;
    }
    let value = if earliest == latest {
        FieldSummary::Same(earliest)
    } else {
        FieldSummary::Mixed
    };
    Ok((value, u64::try_from(modified).unwrap_or(0)))
}
