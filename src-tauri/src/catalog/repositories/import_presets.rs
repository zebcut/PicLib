//! Préréglages d'import : un nom et un contenu
//! JSON (tous les réglages de l'écran d'import), que les commandes
//! vérifient à la lecture.

use rusqlite::{Connection, params};

use crate::catalog::CatalogError;
use crate::domain::keyword_name_key;

/// Préréglage enregistré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredImportPreset {
    /// Identifiant.
    pub id: i64,
    /// Nom.
    pub name: String,
    /// Contenu (JSON).
    pub content: String,
}

/// Tous les préréglages, par ordre alphabétique.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<StoredImportPreset>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id, name, content FROM import_presets ORDER BY name_key")?
        .query_map([], |row| {
            Ok(StoredImportPreset {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

/// Enregistre un préréglage sous `name` (un préréglage du même nom, sans
/// tenir compte des majuscules, est remplacé : c'est le geste « mettre à
/// jour »).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn put(connection: &Connection, name: &str, content: &str) -> Result<(), CatalogError> {
    connection.execute(
        "INSERT INTO import_presets (name, name_key, content) VALUES (?1, ?2, ?3)
         ON CONFLICT (name_key) DO UPDATE SET name = excluded.name, content = excluded.content",
        params![name, keyword_name_key(name), content],
    )?;
    Ok(())
}

/// Supprime un préréglage.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: i64) -> Result<(), CatalogError> {
    connection.execute("DELETE FROM import_presets WHERE id = ?1", [id])?;
    Ok(())
}

/// Préréglage `id`, s'il existe.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find(connection: &Connection, id: i64) -> Result<Option<StoredImportPreset>, CatalogError> {
    Ok(list(connection)?.into_iter().find(|preset| preset.id == id))
}
