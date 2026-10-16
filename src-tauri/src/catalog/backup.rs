//! Sauvegarde vérifiée du catalogue.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::BackupError;
use super::connection::PICLIB_APPLICATION_ID;
use crate::files::unique_path;

/// Début du nom des sauvegardes automatiques (`Catalogue-auto-…`), qui les
/// distingue des sauvegardes faites avant une migration (`Catalogue-v4-…`).
pub const AUTOMATIC_BACKUP_PREFIX: &str = "Catalogue-auto";

/// Sauvegarde faite avant une migration : le nom indique la version de
/// structure sauvegardée et l'heure locale
/// (`Catalogue-v1-20260923-101500.sqlite`).
///
/// # Errors
///
/// Voir [`create_verified_copy`].
pub fn create_verified_backup(
    connection: &Connection,
    backups_dir: &Path,
    schema_version: u32,
) -> Result<PathBuf, BackupError> {
    create_verified_copy(
        connection,
        backups_dir,
        &format!("Catalogue-v{schema_version}"),
        schema_version,
    )
}

/// Sauvegarde automatique (`Catalogue-auto-20260924-183000.sqlite`).
///
/// # Errors
///
/// Voir [`create_verified_copy`].
pub fn create_automatic_backup(
    connection: &Connection,
    backups_dir: &Path,
    schema_version: u32,
) -> Result<PathBuf, BackupError> {
    create_verified_copy(
        connection,
        backups_dir,
        AUTOMATIC_BACKUP_PREFIX,
        schema_version,
    )
}

impl super::Catalog {
    /// Verified backup made before a cleanup
    /// (`Catalogue-cleanup-20250101-120000.sqlite`). Like the backups made
    /// before a migration, it is never pruned automatically.
    ///
    /// # Errors
    ///
    /// Voir [`create_verified_copy`].
    pub fn back_up_before_cleanup(&self, backups_dir: &Path) -> Result<PathBuf, BackupError> {
        create_verified_copy(
            self.connection(),
            backups_dir,
            "Catalogue-cleanup",
            self.schema_version(),
        )
    }
}

/// Copie le catalogue dans `backups_dir` sous le nom `<prefix>-<heure
/// locale>.sqlite`, puis vérifie la copie. Un fichier existant n'est jamais
/// écrasé : un suffixe `-2`, `-3`... est ajouté si besoin.
///
/// # Errors
///
/// Renvoie une erreur si le dossier ne peut pas être créé, si la copie échoue
/// ou si la copie obtenue n'est pas fidèle.
fn create_verified_copy(
    connection: &Connection,
    backups_dir: &Path,
    prefix: &str,
    schema_version: u32,
) -> Result<PathBuf, BackupError> {
    std::fs::create_dir_all(backups_dir).map_err(|source| BackupError::CreateDirectory {
        path: backups_dir.to_path_buf(),
        source,
    })?;

    let stamp: String = connection
        .query_row(
            "SELECT strftime('%Y%m%d-%H%M%S', 'now', 'localtime')",
            [],
            |row| row.get(0),
        )
        .map_err(|source| BackupError::Copy {
            path: backups_dir.to_path_buf(),
            source,
        })?;
    let path = unique_path(backups_dir, &format!("{prefix}-{stamp}"), "sqlite");
    let path_text = path
        .to_str()
        .ok_or_else(|| BackupError::NonUtf8Path(path.clone()))?;

    // « VACUUM INTO » écrit une copie complète et cohérente de la base, même
    // pendant son utilisation, et refuse d'écrire dans un fichier existant.
    connection
        .execute("VACUUM INTO ?1", [path_text])
        .map_err(|source| BackupError::Copy {
            path: path.clone(),
            source,
        })?;

    verify(&path, schema_version)?;
    tracing::info!(path = %path.display(), schema_version, "catalog backup created");
    Ok(path)
}

/// Rouvre la copie en lecture seule et contrôle son intégrité, sa version et
/// son identité de catalogue PicLib.
fn verify(path: &Path, expected_version: u32) -> Result<(), BackupError> {
    let fail = |detail: String| BackupError::Verification {
        path: path.to_path_buf(),
        detail,
    };

    let backup = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| fail(error.to_string()))?;
    let read = || -> rusqlite::Result<(String, u32, i32)> {
        let integrity = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let version = backup.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let application_id = backup.pragma_query_value(None, "application_id", |row| row.get(0))?;
        Ok((integrity, version, application_id))
    };
    let (integrity, version, application_id) = read().map_err(|error| fail(error.to_string()))?;

    if integrity != "ok" {
        return Err(fail(format!("integrity check reported: {integrity}")));
    }
    if version != expected_version {
        return Err(fail(format!(
            "schema version {version} instead of {expected_version}"
        )));
    }
    if application_id != PICLIB_APPLICATION_ID {
        return Err(fail(format!("unexpected application id {application_id}")));
    }
    Ok(())
}
