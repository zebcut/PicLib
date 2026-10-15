//! Fichiers manquants : l'état noté par la
//! vérification de fond, et ce qu'elle lit pour la faire.

use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{Connection, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders, size_from_sql, text_to_path};
use crate::domain::MediaId;

/// Emplacement d'un fichier du catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLocation {
    /// Média.
    pub id: MediaId,
    /// Chemin complet du dossier.
    pub folder: PathBuf,
    /// Nom du fichier.
    pub file_name: String,
}

/// Jusqu'à `limit` emplacements, par identifiant croissant à partir de
/// celui qui suit `after` (lecture par morceaux, sans tenir le catalogue).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn locations_after(
    connection: &Connection,
    after: Option<MediaId>,
    limit: usize,
) -> Result<Vec<MediaLocation>, CatalogError> {
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = connection
        .prepare_cached(
            "SELECT media.id, folders.path, media.file_name
             FROM media JOIN folders ON folders.id = media.folder_id
             WHERE media.id > ?1 ORDER BY media.id LIMIT ?2",
        )?
        .query_map(params![after.map_or(0, MediaId::get), limit], |row| {
            Ok(MediaLocation {
                id: MediaId::new(row.get(0)?),
                folder: text_to_path(row.get::<_, String>(1)?),
                file_name: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Médias notés manquants.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn missing_ids(connection: &Connection) -> Result<HashSet<MediaId>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id FROM media WHERE file_missing = 1")?
        .query_map([], |row| row.get(0).map(MediaId::new))?
        .collect::<Result<_, _>>()?)
}

/// Nombre de médias notés manquants.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count(connection: &Connection) -> Result<u64, CatalogError> {
    let count: i64 = connection
        .prepare_cached("SELECT count(*) FROM media WHERE file_missing = 1")?
        .query_row([], |row| row.get(0))?;
    size_from_sql(count)
}

/// Note les médias `ids` manquants (`missing`) ou présents.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn mark(connection: &Connection, ids: &[MediaId], missing: bool) -> Result<(), CatalogError> {
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "UPDATE media SET file_missing = ?1 WHERE id IN ({})",
            placeholders(2, chunk.len())
        );
        let values = std::iter::once(i64::from(missing)).chain(chunk.iter().map(|id| id.get()));
        connection
            .prepare_cached(&sql)?
            .execute(params_from_iter(values))?;
    }
    Ok(())
}
