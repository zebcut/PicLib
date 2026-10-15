//! Erreurs du catalogue.

use std::path::PathBuf;

/// Erreur lors d'une lecture ou d'une écriture dans le catalogue.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// SQLite a signalé une erreur.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// Un chemin ne peut pas être enregistré car il n'est pas en UTF-8.
    #[error("path is not valid UTF-8: {}", .0.display())]
    NonUtf8Path(PathBuf),
    /// Une valeur lue ou à écrire sort du domaine attendu (base modifiée à
    /// la main, ou taille de fichier hors limites).
    #[error("invalid value {value:?} for column {column}")]
    InvalidValue {
        /// Colonne concernée.
        column: &'static str,
        /// Valeur rencontrée.
        value: String,
    },
    /// L'élément à modifier n'existe pas dans le catalogue.
    #[error("{entity} {id} not found in the catalog")]
    NotFound {
        /// Nature de l'élément (« import »...).
        entity: &'static str,
        /// Identifiant cherché.
        id: i64,
    },
}

/// Erreur lors de la sauvegarde du catalogue.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// Le dossier des sauvegardes n'a pas pu être créé.
    #[error("cannot create the backup directory {}: {source}", path.display())]
    CreateDirectory {
        /// Dossier des sauvegardes.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// La copie a échoué.
    #[error("cannot copy the catalog to {}: {source}", path.display())]
    Copy {
        /// Fichier de sauvegarde visé.
        path: PathBuf,
        /// Erreur SQLite.
        source: rusqlite::Error,
    },
    /// La copie existe mais n'est pas fidèle ou pas lisible.
    #[error("backup {} failed verification: {detail}", path.display())]
    Verification {
        /// Fichier de sauvegarde.
        path: PathBuf,
        /// Ce qui ne va pas.
        detail: String,
    },
    /// Le chemin de la sauvegarde n'est pas en UTF-8.
    #[error("backup path is not valid UTF-8: {}", .0.display())]
    NonUtf8Path(PathBuf),
}

/// Erreur à l'ouverture du catalogue.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// Le catalogue devait exister mais est introuvable.
    #[error("catalog {} not found", path.display())]
    Missing {
        /// Emplacement attendu du catalogue.
        path: PathBuf,
    },
    /// Le fichier ne peut pas être lu comme une base SQLite.
    #[error("cannot read the catalog {}: {source}", path.display())]
    Unreadable {
        /// Fichier du catalogue.
        path: PathBuf,
        /// Erreur SQLite.
        source: rusqlite::Error,
    },
    /// Le fichier est une base SQLite, mais pas un catalogue PicLib.
    #[error("{} is not a PicLib catalog", path.display())]
    NotAPicLibCatalog {
        /// Fichier concerné.
        path: PathBuf,
    },
    /// Le catalogue vient d'une version plus récente de PicLib.
    #[error("catalog {} has schema version {found}, newer than the supported {supported}", path.display())]
    TooNew {
        /// Fichier du catalogue.
        path: PathBuf,
        /// Version trouvée dans le catalogue.
        found: u32,
        /// Version la plus récente connue de cette version de PicLib.
        supported: u32,
    },
    /// La sauvegarde préalable à une migration a échoué : aucune migration
    /// n'a été tentée.
    #[error("backup before migration failed: {0}")]
    Backup(#[from] BackupError),
    /// Une migration a échoué ; le catalogue est resté dans sa version
    /// précédente.
    #[error("migration to schema version {version} failed: {source}")]
    Migration {
        /// Version visée par la migration en échec.
        version: u32,
        /// Erreur SQLite.
        source: rusqlite::Error,
        /// Sauvegarde faite juste avant (absente pour un catalogue neuf).
        backup: Option<PathBuf>,
    },
}
