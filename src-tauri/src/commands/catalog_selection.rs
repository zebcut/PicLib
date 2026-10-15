//! Commandes pour ouvrir un autre catalogue ou en créer un nouveau.
//!
//! Les fenêtres de sélection du système sont ouvertes ici, par le cœur :
//! l'interface ne transmet jamais de chemin, seulement les textes traduits à
//! afficher. En cas de succès, l'application redémarre sur le nouveau
//! catalogue et la commande ne rend jamais la main.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::dialogs;
use crate::catalog::OpenError;
use crate::catalog_selection::{self, SelectionError};
use crate::lifecycle;
use crate::platform;
use crate::settings;
use crate::workspace::Workspace;

/// Emplacement choisi pour un nouveau catalogue, en attente de confirmation
/// par l'utilisateur (dossier synchronisé). Conservé côté cœur pour que
/// l'interface n'ait jamais à renvoyer de chemin.
#[derive(Debug, Default)]
pub struct PendingCatalogCreation(Mutex<Option<PathBuf>>);

/// Issue de « Ouvrir un catalogue » quand l'application ne redémarre pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum OpenCatalogOutcome {
    /// L'utilisateur a fermé la fenêtre de sélection.
    Cancelled,
    /// Le dossier choisi est le catalogue déjà ouvert.
    AlreadyOpen,
}

/// Issue de « Créer un catalogue » quand l'application ne redémarre pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum CreateCatalogOutcome {
    /// L'utilisateur a fermé la fenêtre de sélection.
    Cancelled,
    /// L'emplacement est synchronisé : confirmation demandée avant création.
    SyncedLocation {
        /// Emplacement choisi.
        path: String,
        /// Même emplacement suivi de `.nosync`, conseillé.
        #[serde(rename = "suggestedPath")]
        suggested_path: String,
    },
}

/// Raison pour laquelle le changement de catalogue n'a pas eu lieu. Le
/// catalogue actuel reste alors ouvert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CatalogSelectionFailure {
    /// Les dossiers standard du système sont introuvables.
    SystemDirectories,
    /// La fenêtre de sélection a renvoyé un emplacement inutilisable.
    UnsupportedLocation,
    /// Le dossier choisi ne contient pas de catalogue PicLib.
    NotAPicLibFolder {
        /// Dossier choisi.
        path: String,
    },
    /// Le fichier du catalogue n'est pas un catalogue PicLib.
    NotAPicLibCatalog {
        /// Fichier concerné.
        path: String,
    },
    /// Le catalogue est illisible ou endommagé.
    CatalogUnreadable {
        /// Fichier concerné.
        path: String,
    },
    /// Le catalogue vient d'une version plus récente de PicLib.
    CatalogTooNew {
        /// Fichier concerné.
        path: String,
        /// Version du catalogue.
        found: u32,
        /// Version maximale gérée par cette version de PicLib.
        supported: u32,
    },
    /// Un élément porte déjà ce nom.
    AlreadyExists {
        /// Emplacement demandé.
        path: String,
        /// Vrai si c'est déjà un dossier de travail PicLib.
        #[serde(rename = "containsCatalog")]
        contains_catalog: bool,
    },
    /// Le nouveau catalogue n'a pas pu être créé.
    CreationFailed {
        /// Emplacement demandé.
        path: String,
    },
    /// Le choix n'a pas pu être enregistré dans les réglages.
    SettingsNotSaved {
        /// Fichier de réglages.
        path: String,
    },
    /// Aucune création n'attend de confirmation.
    NoPendingCreation,
}

/// Fait choisir un dossier de travail existant, le vérifie, puis redémarre
/// l'application sur ce catalogue.
///
/// # Errors
///
/// Renvoie la raison de l'échec ; le catalogue actuel reste ouvert.
#[tauri::command]
#[specta::specta]
pub async fn open_catalog(
    app: AppHandle,
    dialog_title: String,
) -> Result<OpenCatalogOutcome, CatalogSelectionFailure> {
    let start_dir = current_workspace(&app)
        .and_then(|workspace| workspace.root().parent().map(Path::to_path_buf));
    let Some(root) = dialogs::pick_folder(&app, dialog_title, start_dir)
        .map_err(|_| CatalogSelectionFailure::UnsupportedLocation)?
    else {
        return Ok(OpenCatalogOutcome::Cancelled);
    };

    if current_workspace(&app).is_some_and(|workspace| workspace.root() == root) {
        return Ok(OpenCatalogOutcome::AlreadyOpen);
    }
    let workspace = catalog_selection::check_existing_workspace(&root)
        .map_err(|error| failure(&error, &root))?;
    Err(switch_to(&app, &workspace))
}

/// Fait choisir l'emplacement et le nom d'un nouveau catalogue, le crée, puis
/// redémarre l'application dessus. Si l'emplacement est synchronisé, demande
/// d'abord confirmation (voir [`confirm_catalog_creation`]).
///
/// # Errors
///
/// Renvoie la raison de l'échec ; le catalogue actuel reste ouvert.
#[tauri::command]
#[specta::specta]
pub async fn create_catalog(
    app: AppHandle,
    pending: State<'_, PendingCatalogCreation>,
    dialog_title: String,
    default_name: String,
) -> Result<CreateCatalogOutcome, CatalogSelectionFailure> {
    let documents_dir = app
        .path()
        .document_dir()
        .map_err(|_| CatalogSelectionFailure::SystemDirectories)?;
    let Some(chosen) = app
        .dialog()
        .file()
        .set_title(dialog_title)
        .set_file_name(default_name)
        .set_directory(documents_dir)
        .set_can_create_directories(true)
        .blocking_save_file()
    else {
        return Ok(CreateCatalogOutcome::Cancelled);
    };
    let root = chosen
        .into_path()
        .map_err(|_| CatalogSelectionFailure::UnsupportedLocation)?;

    catalog_selection::check_new_location(&root).map_err(|error| failure(&error, &root))?;
    if platform::is_synced_location(&root) {
        let suggested = catalog_selection::with_nosync_suffix(&root);
        *pending.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(root.clone());
        return Ok(CreateCatalogOutcome::SyncedLocation {
            path: display(&root),
            suggested_path: display(&suggested),
        });
    }
    Err(create_and_switch(&app, &root))
}

/// Crée le catalogue en attente de confirmation, à l'emplacement choisi ou
/// suivi de `.nosync`, puis redémarre l'application dessus.
///
/// # Errors
///
/// Renvoie la raison de l'échec ; le catalogue actuel reste ouvert.
#[tauri::command]
#[specta::specta]
pub async fn confirm_catalog_creation(
    app: AppHandle,
    pending: State<'_, PendingCatalogCreation>,
    add_nosync_suffix: bool,
) -> Result<(), CatalogSelectionFailure> {
    let chosen = pending
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    let Some(chosen) = chosen else {
        return Err(CatalogSelectionFailure::NoPendingCreation);
    };
    let root = if add_nosync_suffix {
        catalog_selection::with_nosync_suffix(&chosen)
    } else {
        chosen
    };
    Err(create_and_switch(&app, &root))
}

/// Crée le dossier de travail puis bascule dessus. Ne rend la main qu'en cas
/// d'échec.
fn create_and_switch(app: &AppHandle, root: &Path) -> CatalogSelectionFailure {
    match catalog_selection::create_workspace(root) {
        Ok(workspace) => switch_to(app, &workspace),
        Err(error) => failure(&error, root),
    }
}

/// Enregistre le nouveau dossier de travail puis redémarre l'application.
/// Ne rend la main qu'en cas d'échec.
fn switch_to(app: &AppHandle, workspace: &Workspace) -> CatalogSelectionFailure {
    let Ok(config_dir) = app.path().app_config_dir() else {
        return CatalogSelectionFailure::SystemDirectories;
    };
    let settings_path = settings::settings_file(&config_dir);
    match catalog_selection::remember_workspace(&settings_path, workspace) {
        Ok(()) => lifecycle::restart(app),
        Err(error) => failure(&error, &settings_path),
    }
}

/// Dossier de travail ouvert, si le démarrage a réussi.
fn current_workspace(app: &AppHandle) -> Option<Workspace> {
    app.try_state::<Workspace>()
        .map(|workspace| workspace.inner().clone())
}

/// Traduit une erreur du service en raison à montrer, et la journalise.
/// `location` est l'emplacement concerné quand l'erreur n'en porte pas.
fn failure(error: &SelectionError, location: &Path) -> CatalogSelectionFailure {
    tracing::warn!(%error, "catalog selection failed");
    match error {
        SelectionError::NotAWorkspace { path }
        | SelectionError::Catalog(OpenError::Missing { path }) => {
            CatalogSelectionFailure::NotAPicLibFolder {
                path: display(path),
            }
        }
        SelectionError::Catalog(OpenError::NotAPicLibCatalog { path }) => {
            CatalogSelectionFailure::NotAPicLibCatalog {
                path: display(path),
            }
        }
        SelectionError::Catalog(OpenError::Unreadable { path, .. }) => {
            CatalogSelectionFailure::CatalogUnreadable {
                path: display(path),
            }
        }
        SelectionError::Catalog(OpenError::TooNew {
            path,
            found,
            supported,
        }) => CatalogSelectionFailure::CatalogTooNew {
            path: display(path),
            found: *found,
            supported: *supported,
        },
        SelectionError::AlreadyExists {
            path,
            contains_catalog,
        } => CatalogSelectionFailure::AlreadyExists {
            path: display(path),
            contains_catalog: *contains_catalog,
        },
        SelectionError::Catalog(OpenError::Backup(_) | OpenError::Migration { .. })
        | SelectionError::CreateDirectory { .. } => CatalogSelectionFailure::CreationFailed {
            path: display(location),
        },
        SelectionError::Settings(_) | SelectionError::SetAsideSettings { .. } => {
            CatalogSelectionFailure::SettingsNotSaved {
                path: display(location),
            }
        }
    }
}

fn display(path: &Path) -> String {
    path.display().to_string()
}
