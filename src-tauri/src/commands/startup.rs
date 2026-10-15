//! État du démarrage, transmis à l'interface.
//!
//! L'interface affiche l'écran d'accueil si le catalogue est prêt, ou un écran
//! d'erreur traduit sinon. Seuls la nature de l'erreur et les chemins utiles
//! sont transmis ; le détail technique est dans le journal.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::catalog::{BackupError, OpenError};
use crate::startup::{OpenedWorkspace, StartupError};

/// Résultat de l'ouverture du catalogue au démarrage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum StartupStatus {
    /// Le catalogue est ouvert et à jour.
    Ready {
        /// Dossier de travail.
        #[serde(rename = "workspaceDir")]
        workspace_dir: String,
        /// Version de structure du catalogue.
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
    },
    /// Le catalogue n'a pas pu être ouvert.
    Failed {
        /// Nature de l'erreur.
        failure: StartupFailure,
    },
}

/// Nature d'un échec au démarrage, avec les informations à montrer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartupFailure {
    /// Dossiers du système introuvables.
    SystemDirectories,
    /// Fichier de réglages illisible.
    SettingsUnreadable {
        /// Fichier de réglages.
        path: String,
    },
    /// Dossier de travail absent (disque débranché, dossier déplacé...).
    WorkspaceUnavailable {
        /// Dossier de travail attendu.
        path: String,
    },
    /// Dossier de travail impossible à créer.
    WorkspaceCreationFailed {
        /// Dossier de travail visé.
        path: String,
    },
    /// Fichier du catalogue absent du dossier de travail.
    CatalogMissing {
        /// Emplacement attendu.
        path: String,
    },
    /// Catalogue illisible ou endommagé.
    CatalogUnreadable {
        /// Fichier du catalogue.
        path: String,
    },
    /// Le fichier n'est pas un catalogue PicLib.
    NotAPicLibCatalog {
        /// Fichier concerné.
        path: String,
    },
    /// Catalogue créé par une version plus récente de PicLib.
    CatalogTooNew {
        /// Fichier du catalogue.
        path: String,
        /// Version du catalogue.
        found: u32,
        /// Version maximale gérée par cette version de PicLib.
        supported: u32,
    },
    /// Sauvegarde préalable à la mise à jour impossible.
    BackupFailed {
        /// Sauvegarde ou dossier des sauvegardes concerné.
        path: String,
    },
    /// Mise à jour du catalogue en échec.
    MigrationFailed {
        /// Sauvegarde faite juste avant, s'il y en a une.
        backup: Option<String>,
    },
}

impl StartupStatus {
    /// État d'un démarrage réussi.
    #[must_use]
    pub fn ready(opened: &OpenedWorkspace) -> Self {
        Self::Ready {
            workspace_dir: display(opened.workspace.root()),
            schema_version: opened.catalog.schema_version(),
        }
    }

    /// État d'un démarrage en échec.
    #[must_use]
    pub fn failed(error: &StartupError) -> Self {
        Self::Failed {
            failure: StartupFailure::from(error),
        }
    }
}

impl From<&StartupError> for StartupFailure {
    fn from(error: &StartupError) -> Self {
        match error {
            StartupError::SystemDirectories(_) => Self::SystemDirectories,
            StartupError::Settings { path, .. } => Self::SettingsUnreadable {
                path: display(path),
            },
            StartupError::WorkspaceUnavailable { path } => Self::WorkspaceUnavailable {
                path: display(path),
            },
            StartupError::WorkspaceCreation { path, .. } => Self::WorkspaceCreationFailed {
                path: display(path),
            },
            StartupError::Catalog(error) => Self::from(error),
        }
    }
}

impl From<&OpenError> for StartupFailure {
    fn from(error: &OpenError) -> Self {
        match error {
            OpenError::Missing { path } => Self::CatalogMissing {
                path: display(path),
            },
            OpenError::Unreadable { path, .. } => Self::CatalogUnreadable {
                path: display(path),
            },
            OpenError::NotAPicLibCatalog { path } => Self::NotAPicLibCatalog {
                path: display(path),
            },
            OpenError::TooNew {
                path,
                found,
                supported,
            } => Self::CatalogTooNew {
                path: display(path),
                found: *found,
                supported: *supported,
            },
            OpenError::Backup(error) => Self::BackupFailed {
                path: display(backup_error_path(error)),
            },
            OpenError::Migration { backup, .. } => Self::MigrationFailed {
                backup: backup.as_deref().map(display),
            },
        }
    }
}

fn backup_error_path(error: &BackupError) -> &Path {
    match error {
        BackupError::CreateDirectory { path, .. }
        | BackupError::Copy { path, .. }
        | BackupError::Verification { path, .. }
        | BackupError::NonUtf8Path(path) => path,
    }
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

/// Renvoie l'état du démarrage (catalogue prêt ou nature de l'erreur).
// `State` est fourni par Tauri et doit être reçu par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_startup_status(status: State<'_, StartupStatus>) -> StartupStatus {
    status.inner().clone()
}
