//! Changer de catalogue : contrôler un dossier de travail existant, en créer
//! un nouveau, et retenir le choix dans les réglages.
//!
//! Un seul catalogue est ouvert à la fois. Après un changement, l'application
//! redémarre sur le nouveau catalogue (voir [`crate::lifecycle`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, OpenError, OpenMode};
use crate::files::unique_path;
use crate::settings::{self, Settings, SettingsError};
use crate::workspace::Workspace;

/// Suffixe qui exclut un dossier de la synchronisation iCloud Drive.
const NOSYNC_SUFFIX: &str = ".nosync";

/// Erreur lors d'un changement de catalogue.
#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    /// Le dossier choisi ne contient pas de catalogue PicLib.
    #[error("{} does not contain a PicLib catalog", path.display())]
    NotAWorkspace {
        /// Dossier choisi.
        path: PathBuf,
    },
    /// Le catalogue ne peut pas être ouvert ou créé.
    #[error(transparent)]
    Catalog(#[from] OpenError),
    /// Un élément porte déjà ce nom : il n'est jamais réutilisé ni écrasé.
    #[error("{} already exists", path.display())]
    AlreadyExists {
        /// Emplacement demandé.
        path: PathBuf,
        /// Vrai si c'est déjà un dossier de travail PicLib (à ouvrir plutôt).
        contains_catalog: bool,
    },
    /// Le dossier n'a pas pu être créé.
    #[error("cannot create {}: {source}", path.display())]
    CreateDirectory {
        /// Dossier à créer.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// Les réglages n'ont pas pu être enregistrés.
    #[error("cannot save the settings: {0}")]
    Settings(#[from] SettingsError),
    /// Un fichier de réglages illisible n'a pas pu être mis de côté.
    #[error("cannot set aside the unreadable settings file {}: {source}", path.display())]
    SetAsideSettings {
        /// Fichier de réglages illisible.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
}

/// Vérifie, sans rien modifier, que `root` est un dossier de travail dont le
/// catalogue peut être ouvert par cette version de PicLib.
///
/// # Errors
///
/// [`SelectionError::NotAWorkspace`] si le dossier ne contient pas de
/// catalogue, [`SelectionError::Catalog`] s'il est illisible, étranger ou trop
/// récent.
pub fn check_existing_workspace(root: &Path) -> Result<Workspace, SelectionError> {
    let workspace = Workspace::new(root.to_path_buf());
    match Catalog::inspect(&workspace) {
        Ok(_) => Ok(workspace),
        Err(OpenError::Missing { .. }) => Err(SelectionError::NotAWorkspace {
            path: root.to_path_buf(),
        }),
        Err(error) => Err(error.into()),
    }
}

/// Vérifie qu'aucun élément ne porte déjà le nom `root`, avant de proposer
/// de créer un catalogue à cet emplacement.
///
/// # Errors
///
/// [`SelectionError::AlreadyExists`] si l'emplacement est pris.
pub fn check_new_location(root: &Path) -> Result<(), SelectionError> {
    if root.exists() {
        return Err(SelectionError::AlreadyExists {
            path: root.to_path_buf(),
            contains_catalog: Workspace::new(root.to_path_buf()).catalog_path().is_file(),
        });
    }
    Ok(())
}

/// Crée un nouveau dossier de travail à `root` avec un catalogue vide.
///
/// Le dossier ne doit pas exister : un dossier existant n'est jamais réutilisé,
/// même vide, pour ne rien mélanger avec son contenu.
///
/// # Errors
///
/// [`SelectionError::AlreadyExists`] si `root` existe déjà, sinon l'erreur de
/// création du dossier ou du catalogue.
pub fn create_workspace(root: &Path) -> Result<Workspace, SelectionError> {
    let workspace = Workspace::new(root.to_path_buf());
    // `create_dir` (et non `create_dir_all`) échoue si le dossier existe :
    // aucun risque de réutiliser un dossier créé entre-temps.
    if let Err(source) = std::fs::create_dir(root) {
        return Err(if source.kind() == std::io::ErrorKind::AlreadyExists {
            SelectionError::AlreadyExists {
                path: root.to_path_buf(),
                contains_catalog: workspace.catalog_path().is_file(),
            }
        } else {
            SelectionError::CreateDirectory {
                path: root.to_path_buf(),
                source,
            }
        });
    }
    // La connexion est fermée aussitôt : le catalogue sera rouvert au
    // redémarrage de l'application.
    drop(Catalog::open(&workspace, OpenMode::CreateIfMissing)?);
    tracing::info!(path = %root.display(), "new workspace created");
    Ok(workspace)
}

/// Enregistre `workspace` comme dossier de travail à ouvrir au prochain
/// démarrage.
///
/// Un fichier de réglages illisible est renommé (`settings.invalid.json`,
/// `settings.invalid-2.json`...) plutôt qu'écrasé : il contenait peut-être
/// l'emplacement d'un autre catalogue.
///
/// # Errors
///
/// Échoue si les réglages ne peuvent pas être mis de côté ou enregistrés.
pub fn remember_workspace(
    settings_path: &Path,
    workspace: &Workspace,
) -> Result<(), SelectionError> {
    // Les préférences sont gardées : seul le dossier de travail change.
    let preferences = match settings::load(settings_path) {
        Ok(settings) => settings.map(|settings| settings.preferences),
        Err(error) => {
            set_aside_unreadable_settings(settings_path, &error)?;
            None
        }
    };
    let mut settings = Settings::new(workspace.root().to_path_buf());
    if let Some(preferences) = preferences {
        settings.preferences = preferences;
    }
    settings::save(settings_path, &settings)?;
    tracing::info!(path = %workspace.root().display(), "workspace selected for the next start");
    Ok(())
}

fn set_aside_unreadable_settings(
    settings_path: &Path,
    error: &SettingsError,
) -> Result<(), SelectionError> {
    let directory = settings_path.parent().unwrap_or_else(|| Path::new("."));
    let aside = unique_path(directory, "settings.invalid", "json");
    std::fs::rename(settings_path, &aside).map_err(|source| SelectionError::SetAsideSettings {
        path: settings_path.to_path_buf(),
        source,
    })?;
    tracing::warn!(%error, aside = %aside.display(), "unreadable settings file set aside");
    Ok(())
}

/// Ajoute `.nosync` au nom du dossier, s'il ne l'a pas déjà, pour l'exclure
/// de la synchronisation iCloud Drive.
#[must_use]
pub fn with_nosync_suffix(path: &Path) -> PathBuf {
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };
    if name.to_string_lossy().ends_with(NOSYNC_SUFFIX) {
        return path.to_path_buf();
    }
    let mut new_name = OsString::from(name);
    new_name.push(NOSYNC_SUFFIX);
    path.with_file_name(new_name)
}
