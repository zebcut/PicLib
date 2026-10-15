//! Derniers dossiers choisis dans l'écran d'import, retenus
//! d'une session à l'autre ; l'utilisateur peut en retirer un de la liste
//! (le dossier lui-même n'est pas touché).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{path_to_text, text_to_path};
use crate::domain::Timestamp;

/// Nombre de dossiers gardés dans la liste.
pub const RECENT_LIMIT: usize = 10;

/// Note `path` comme choisi à l'instant ; seuls les plus récents sont
/// gardés.
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn touch(connection: &Connection, path: &Path, now: Timestamp) -> Result<(), CatalogError> {
    connection.execute(
        "INSERT INTO recent_import_folders (path, used_at) VALUES (?1, ?2)
         ON CONFLICT (path) DO UPDATE SET used_at = excluded.used_at",
        params![path_to_text(path)?, now.unix_millis()],
    )?;
    connection.execute(
        "DELETE FROM recent_import_folders WHERE path NOT IN
             (SELECT path FROM recent_import_folders ORDER BY used_at DESC LIMIT ?1)",
        [i64::try_from(RECENT_LIMIT).unwrap_or(i64::MAX)],
    )?;
    Ok(())
}

/// Dossiers, du plus récent au plus ancien.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<PathBuf>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT path FROM recent_import_folders ORDER BY used_at DESC")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(text_to_path)
        .collect())
}

/// Retire un dossier de la liste.
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn forget(connection: &Connection, path: &Path) -> Result<(), CatalogError> {
    connection.execute(
        "DELETE FROM recent_import_folders WHERE path = ?1",
        [path_to_text(path)?],
    )?;
    Ok(())
}
