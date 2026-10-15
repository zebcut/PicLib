//! Préréglages de filtres de l'utilisateur : un nom unique et
//! un contenu JSON, que les commandes vérifient à la lecture.

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::domain::{FilterPresetId, keyword_name_key};

/// Préréglage de filtres enregistré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPreset {
    /// Identifiant.
    pub id: FilterPresetId,
    /// Nom.
    pub name: String,
    /// Contenu (onglets, critères, colonnes), en JSON.
    pub content: String,
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<FilterPreset> {
    Ok(FilterPreset {
        id: FilterPresetId::new(row.get(0)?),
        name: row.get(1)?,
        content: row.get(2)?,
    })
}

/// Tous les préréglages, par ordre alphabétique.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<FilterPreset>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id, name, content FROM filter_presets ORDER BY name_key")?
        .query_map([], read)?
        .collect::<Result<_, _>>()?)
}

/// Un préréglage ; `None` s'il n'existe pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn get(
    connection: &Connection,
    id: FilterPresetId,
) -> Result<Option<FilterPreset>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id, name, content FROM filter_presets WHERE id = ?1")?
        .query_row([id.get()], read)
        .optional()?)
}

/// Préréglage qui porte ce nom (sans tenir compte des majuscules).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_by_name(
    connection: &Connection,
    name: &str,
) -> Result<Option<FilterPresetId>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id FROM filter_presets WHERE name_key = ?1")?
        .query_row([keyword_name_key(name)], |row| row.get(0))
        .optional()?
        .map(FilterPresetId::new))
}

/// Identifiant d'un nouveau préréglage, choisi avant de le créer : une
/// annulation puis un rétablissement le recréent sous le même.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn next_id(connection: &Connection) -> Result<FilterPresetId, CatalogError> {
    let id: i64 = connection
        .prepare_cached("SELECT coalesce(max(id), 0) + 1 FROM filter_presets")?
        .query_row([], |row| row.get(0))?;
    Ok(FilterPresetId::new(id))
}

/// Crée le préréglage, ou remplace celui de même identifiant.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite (nom déjà pris).
pub fn put(connection: &Connection, preset: &FilterPreset) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO filter_presets (id, name, name_key, content) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE
                 SET name = excluded.name, name_key = excluded.name_key,
                     content = excluded.content",
        )?
        .execute(params![
            preset.id.get(),
            preset.name,
            keyword_name_key(&preset.name),
            preset.content
        ])?;
    Ok(())
}

/// Supprime un préréglage (sans effet s'il n'existe pas).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: FilterPresetId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM filter_presets WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}
