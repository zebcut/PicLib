//! Modèles de renommage enregistrés : un nom et un contenu
//! JSON, que les commandes vérifient à la lecture.

use rusqlite::{Connection, params};

use crate::catalog::CatalogError;
use crate::domain::keyword_name_key;

/// Modèle enregistré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRenameTemplate {
    /// Identifiant.
    pub id: i64,
    /// Nom.
    pub name: String,
    /// Contenu (JSON).
    pub content: String,
}

/// Tous les modèles, par ordre alphabétique.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<StoredRenameTemplate>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT id, name, content FROM rename_templates ORDER BY name_key")?
        .query_map([], |row| {
            Ok(StoredRenameTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

/// Enregistre un modèle sous `name` (un modèle du même nom, sans tenir
/// compte des majuscules, est remplacé : c'est le geste « mettre à jour »).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn put(connection: &Connection, name: &str, content: &str) -> Result<(), CatalogError> {
    connection.execute(
        "INSERT INTO rename_templates (name, name_key, content) VALUES (?1, ?2, ?3)
         ON CONFLICT (name_key) DO UPDATE SET name = excluded.name, content = excluded.content",
        params![name, keyword_name_key(name), content],
    )?;
    Ok(())
}

/// Supprime un modèle.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: i64) -> Result<(), CatalogError> {
    connection.execute("DELETE FROM rename_templates WHERE id = ?1", [id])?;
    Ok(())
}
