//! Ouverture et réglages de la connexion SQLite.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use super::OpenError;

/// Identifiant écrit dans l'en-tête de tout catalogue PicLib (`PicL` en
/// ASCII), par la migration 1.
pub const PICLIB_APPLICATION_ID: i32 = 0x5069_634C;

/// Temps d'attente maximal si la base est momentanément verrouillée par une
/// autre connexion, avant d'abandonner avec une erreur.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Ce que contient un fichier au moment de son ouverture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIdentity {
    /// Base vide : catalogue tout neuf.
    Empty,
    /// Catalogue PicLib.
    PicLibCatalog,
}

/// Ouvre la base en lecture-écriture, en la créant si `create` est vrai.
pub fn open(path: &Path, create: bool) -> Result<Connection, OpenError> {
    // Pas d'option « URI » : un chemin est toujours traité comme un chemin.
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    Connection::open_with_flags(path, flags).map_err(|source| unreadable(path, source))
}

/// Ouvre la base en lecture seule, pour l'examiner sans rien y changer.
pub fn open_read_only(path: &Path) -> Result<Connection, OpenError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(path, flags).map_err(|source| unreadable(path, source))
}

/// Vérifie que la base est vide ou est un catalogue PicLib, sans rien y
/// écrire : un fichier étranger ne doit jamais être modifié.
pub fn identify(connection: &Connection, path: &Path) -> Result<FileIdentity, OpenError> {
    let read = || -> rusqlite::Result<(i32, u32, i64)> {
        let application_id =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let user_version = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let object_count =
            connection.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
        Ok((application_id, user_version, object_count))
    };
    let (application_id, user_version, object_count) =
        read().map_err(|source| unreadable(path, source))?;

    match (application_id, user_version, object_count) {
        (PICLIB_APPLICATION_ID, _, _) => Ok(FileIdentity::PicLibCatalog),
        (0, 0, 0) => Ok(FileIdentity::Empty),
        _ => Err(OpenError::NotAPicLibCatalog {
            path: path.to_path_buf(),
        }),
    }
}

/// Applique les réglages de sécurité et de performance de la connexion.
pub fn configure(connection: &Connection, path: &Path) -> Result<(), OpenError> {
    let apply = || -> rusqlite::Result<()> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        // Refuse toute référence vers un élément inexistant (ex. un média
        // dans un dossier absent du catalogue). Désactivé par défaut dans
        // SQLite, et à activer à chaque connexion.
        connection.pragma_update(None, "foreign_keys", true)?;
        // Mode « WAL » : les lectures (affichage) ne sont pas bloquées par une
        // écriture en cours (import), et une coupure ne corrompt pas la base.
        connection.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        // Chaque transaction validée est réellement écrite sur le disque avant
        // de rendre la main : un peu plus lent, mais aucune perte possible.
        connection.pragma_update(None, "synchronous", "FULL")?;
        // `rarray(?)` : une liste d'identifiants passée en paramètre (filtre
        // Texte, voir `repositories::library`).
        rusqlite::vtab::array::load_module(connection)
    };
    apply().map_err(|source| unreadable(path, source))
}

fn unreadable(path: &Path, source: rusqlite::Error) -> OpenError {
    OpenError::Unreadable {
        path: path.to_path_buf(),
        source,
    }
}
