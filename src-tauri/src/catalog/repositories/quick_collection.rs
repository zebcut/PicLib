//! Collection rapide et collection cible.

use rusqlite::{Connection, OptionalExtension, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders, size_from_sql};
use crate::domain::{CollectionId, MediaId};

/// Photos de la collection rapide.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn media(connection: &Connection) -> Result<Vec<MediaId>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT media_id FROM quick_collection ORDER BY media_id")?
        .query_map([], |row| row.get(0).map(MediaId::new))?
        .collect::<Result<_, _>>()?)
}

/// Nombre de photos de la collection rapide.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count(connection: &Connection) -> Result<u64, CatalogError> {
    let count: i64 = connection
        .prepare_cached("SELECT count(*) FROM quick_collection")?
        .query_row([], |row| row.get(0))?;
    size_from_sql(count)
}

/// Parmi `ids`, les photos déjà dans la collection rapide.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn present(connection: &Connection, ids: &[MediaId]) -> Result<Vec<MediaId>, CatalogError> {
    let mut found = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media_id FROM quick_collection WHERE media_id IN ({})",
            placeholders(1, chunk.len())
        );
        found.extend(
            connection
                .prepare_cached(&sql)?
                .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                    row.get(0).map(MediaId::new)
                })?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(found)
}

/// Met ces photos dans la collection rapide (déjà présentes : sans effet).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn add(connection: &Connection, ids: &[MediaId]) -> Result<(), CatalogError> {
    let mut statement = connection
        .prepare_cached("INSERT OR IGNORE INTO quick_collection (media_id) VALUES (?1)")?;
    for id in ids {
        statement.execute([id.get()])?;
    }
    Ok(())
}

/// Retire ces photos de la collection rapide.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn remove(connection: &Connection, ids: &[MediaId]) -> Result<(), CatalogError> {
    let mut statement =
        connection.prepare_cached("DELETE FROM quick_collection WHERE media_id = ?1")?;
    for id in ids {
        statement.execute([id.get()])?;
    }
    Ok(())
}

/// Collection cible ; `None` : la collection rapide.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn target(connection: &Connection) -> Result<Option<CollectionId>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT collection_id FROM collection_target WHERE id = 1")?
        .query_row([], |row| row.get::<_, Option<i64>>(0))
        .optional()?
        .flatten()
        .map(CollectionId::new))
}

/// Choisit la collection cible (`None` : la collection rapide).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_target(connection: &Connection, id: Option<CollectionId>) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO collection_target (id, collection_id) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET collection_id = excluded.collection_id",
        )?
        .execute([id.map(CollectionId::get)])?;
    Ok(())
}

/// Expression SQL vraie si le média (`media.id`) est dans la cible : la
/// grille dessine le rond plein de ses vignettes.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub(super) fn in_target_sql(connection: &Connection) -> Result<String, CatalogError> {
    Ok(match target(connection)? {
        // Un identifiant lu dans la base, jamais une saisie.
        Some(id) => format!(
            "EXISTS (SELECT 1 FROM collection_media
                 WHERE collection_id = {} AND media_id = media.id)",
            id.get()
        ),
        None => "EXISTS (SELECT 1 FROM quick_collection WHERE media_id = media.id)".to_owned(),
    })
}
