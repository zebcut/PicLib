//! Usage des mots-clés, pour les suggestions : date de dernière
//! utilisation (migration 14), mots-clés qui accompagnent ceux d'une photo,
//! mots-clés des photos prises au même moment.

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::domain::{KeywordId, MediaId, Timestamp};

/// Écart autour de la date de prise de vue d'une photo pour trouver les
/// photos prises « au même moment » : d'abord l'heure, puis la journée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeWindow {
    /// Une heure avant ou après.
    Hour,
    /// Le même jour (de minuit à minuit).
    Day,
}

/// Retient que ces mots-clés viennent d'être posés sur des photos.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn mark_used(
    connection: &Connection,
    ids: &[KeywordId],
    at: Timestamp,
) -> Result<(), CatalogError> {
    let mut statement =
        connection.prepare_cached("UPDATE keywords SET last_used_at = ?2 WHERE id = ?1")?;
    for id in ids {
        statement.execute(params![id.get(), at.unix_millis()])?;
    }
    Ok(())
}

/// Les mots-clés posés le plus récemment, du plus récent au plus ancien.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn recently_used(
    connection: &Connection,
    limit: usize,
) -> Result<Vec<KeywordId>, CatalogError> {
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut statement = connection.prepare_cached(
        "SELECT id FROM keywords WHERE last_used_at IS NOT NULL
         ORDER BY last_used_at DESC, id DESC LIMIT ?1",
    )?;
    let ids = statement
        .query_map([limit], |row| Ok(KeywordId::new(row.get(0)?)))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Mots-clés des autres photos qui partagent un mot-clé avec `media`, avec
/// leur nombre d'apparitions (une photo qui partage deux mots-clés compte
/// deux fois : elle ressemble davantage).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn co_occurring(
    connection: &Connection,
    media: MediaId,
) -> Result<Vec<(KeywordId, u32)>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT other.keyword_id, count(*)
         FROM media_keywords AS mine
         JOIN media_keywords AS same
           ON same.keyword_id = mine.keyword_id AND same.media_id <> mine.media_id
         JOIN media_keywords AS other ON other.media_id = same.media_id
         WHERE mine.media_id = ?1
         GROUP BY other.keyword_id",
    )?;
    let counts = statement
        .query_map([media.get()], |row| {
            Ok((KeywordId::new(row.get(0)?), row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(counts)
}

/// Mots-clés des autres photos prises dans `window` autour de `media`, avec
/// le nombre de ces photos qui portent chacun. Vide si la photo n'a pas de
/// date de prise de vue.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn near_in_time(
    connection: &Connection,
    media: MediaId,
    window: TimeWindow,
) -> Result<Vec<(KeywordId, u32)>, CatalogError> {
    let Some(capture_time) = capture_time(connection, media)? else {
        return Ok(Vec::new());
    };
    // Bornes calculées une fois, au format des dates stockées : la
    // recherche suit l'index des dates de prise de vue
    // (`media_by_capture_time`).
    let bounds_sql = match window {
        TimeWindow::Hour => {
            "SELECT strftime('%Y-%m-%dT%H:%M:%S', ?1, '-1 hour'),
                    strftime('%Y-%m-%dT%H:%M:%S', ?1, '+1 hour')"
        }
        TimeWindow::Day => {
            "SELECT strftime('%Y-%m-%dT%H:%M:%S', ?1, 'start of day'),
                    strftime('%Y-%m-%dT%H:%M:%S', ?1, 'start of day', '+1 day', '-1 second')"
        }
    };
    let bounds: Option<(String, String)> = connection
        .prepare_cached(bounds_sql)?
        .query_row([capture_time], |row| {
            Ok(row.get::<_, Option<String>>(0)?.zip(row.get(1)?))
        })?;
    let Some((from, to)) = bounds else {
        return Ok(Vec::new());
    };
    let mut statement = connection.prepare_cached(
        "SELECT media_keywords.keyword_id, count(*)
         FROM media
         JOIN media_keywords ON media_keywords.media_id = media.id
         WHERE media.capture_time >= ?1 AND media.capture_time <= ?2 AND media.id <> ?3
         GROUP BY media_keywords.keyword_id",
    )?;
    let counts = statement
        .query_map(params![from, to, media.get()], |row| {
            Ok((KeywordId::new(row.get(0)?), row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(counts)
}

/// Date de prise de vue d'un média (texte `AAAA-MM-JJTHH:MM:SS`), s'il en a
/// une.
fn capture_time(connection: &Connection, media: MediaId) -> Result<Option<String>, CatalogError> {
    let time: Option<String> = connection
        .prepare_cached("SELECT capture_time FROM media WHERE id = ?1")?
        .query_row([media.get()], |row| row.get(0))
        .optional()?;
    Ok(time.filter(|time| !time.is_empty()))
}
