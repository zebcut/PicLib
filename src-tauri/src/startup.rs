//! Démarrage : lecture des réglages, dossier de travail, ouverture du
//! catalogue.
//!
//! Au premier lancement, le dossier de travail par défaut est créé et
//! enregistré dans les réglages. Ensuite, un dossier de travail ou un
//! catalogue disparu (disque débranché, dossier déplacé) est signalé comme une
//! erreur : il n'est jamais recréé vide, ce qui ferait croire à l'utilisateur
//! que ses données sont perdues.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, Runtime};

use crate::catalog::{Catalog, OpenError, OpenMode};
use crate::platform;
use crate::settings::{self, Settings, SettingsError};
use crate::workspace::Workspace;

/// Dossier de travail et catalogue ouverts.
#[derive(Debug)]
pub struct OpenedWorkspace {
    /// Dossier de travail.
    pub workspace: Workspace,
    /// Catalogue ouvert et à jour.
    pub catalog: Catalog,
}

/// Erreur empêchant d'ouvrir le catalogue au démarrage.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// Le système n'a pas fourni ses dossiers standard (configuration,
    /// Documents).
    #[error("cannot resolve the system directories: {0}")]
    SystemDirectories(#[source] tauri::Error),
    /// Le fichier de réglages est illisible.
    #[error("settings file {} is unusable: {source}", path.display())]
    Settings {
        /// Fichier de réglages.
        path: PathBuf,
        /// Cause.
        source: SettingsError,
    },
    /// Le dossier de travail enregistré n'existe pas ou plus.
    #[error("workspace {} is unavailable", path.display())]
    WorkspaceUnavailable {
        /// Dossier de travail attendu.
        path: PathBuf,
    },
    /// Le dossier de travail par défaut n'a pas pu être créé.
    #[error("cannot create the workspace {}: {source}", path.display())]
    WorkspaceCreation {
        /// Dossier de travail visé.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// Le catalogue n'a pas pu être ouvert.
    #[error(transparent)]
    Catalog(#[from] OpenError),
}

/// Ouvre le dossier de travail et le catalogue de l'application.
///
/// # Errors
///
/// Voir [`StartupError`].
pub fn open_for_app<R: Runtime>(app: &AppHandle<R>) -> Result<OpenedWorkspace, StartupError> {
    let paths = app.path();
    let config_dir = paths
        .app_config_dir()
        .map_err(StartupError::SystemDirectories)?;
    let documents_dir = paths
        .document_dir()
        .map_err(StartupError::SystemDirectories)?;
    open(&config_dir, &documents_dir)
}

/// Ouvre le dossier de travail et le catalogue, à partir du dossier de
/// configuration de l'application et du dossier Documents de l'utilisateur.
///
/// # Errors
///
/// Voir [`StartupError`].
pub fn open(config_dir: &Path, documents_dir: &Path) -> Result<OpenedWorkspace, StartupError> {
    let settings_path = settings::settings_file(config_dir);
    let settings = settings::load(&settings_path).map_err(|source| StartupError::Settings {
        path: settings_path.clone(),
        source,
    })?;

    match settings {
        Some(settings) => open_known_workspace(Workspace::new(settings.workspace_dir)),
        None => open_first_workspace(&settings_path, documents_dir),
    }
}

fn open_known_workspace(workspace: Workspace) -> Result<OpenedWorkspace, StartupError> {
    if !workspace.root().is_dir() {
        return Err(StartupError::WorkspaceUnavailable {
            path: workspace.root().to_path_buf(),
        });
    }
    let catalog = Catalog::open(&workspace, OpenMode::MustExist)?;
    Ok(OpenedWorkspace { workspace, catalog })
}

fn open_first_workspace(
    settings_path: &Path,
    documents_dir: &Path,
) -> Result<OpenedWorkspace, StartupError> {
    let workspace = Workspace::new(platform::default_workspace_dir(documents_dir));
    tracing::info!(path = %workspace.root().display(), "first launch: preparing the default workspace");

    std::fs::create_dir_all(workspace.root()).map_err(|source| {
        StartupError::WorkspaceCreation {
            path: workspace.root().to_path_buf(),
            source,
        }
    })?;
    // Si un catalogue existe déjà à cet endroit (réglages effacés), il est
    // simplement rouvert.
    let catalog = Catalog::open(&workspace, OpenMode::CreateIfMissing)?;

    // Les réglages ne sont enregistrés qu'une fois le catalogue prêt : si
    // quelque chose échoue avant, le prochain lancement recommence proprement.
    // Un échec ici n'empêche pas de travailler : au prochain lancement, le
    // même dossier par défaut sera retrouvé et rouvert.
    if let Err(error) = settings::save(
        settings_path,
        &Settings::new(workspace.root().to_path_buf()),
    ) {
        tracing::error!(%error, "cannot save the settings; the default workspace will be used again");
    }
    Ok(OpenedWorkspace { workspace, catalog })
}
