//! Choix de la fenêtre d'import : derniers choix retenus, et dossier de
//! destination d'une copie. Les chemins restent dans le cœur : l'interface
//! les affiche, mais ne les renvoie jamais.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::dialogs;
use super::edits::lock;
use crate::catalog::Catalog;
use crate::catalog::repositories::import_preferences::{self, ImportPreferences, ImportStacking};
use crate::domain::ImportMode;
use crate::import::DestinationLayout;
use crate::renaming::RenameTemplate;
use crate::workspace::Workspace;

/// Dossiers choisis avec « Choisir… » dans l'écran d'import (un seul écran
/// d'import à la fois) : destination de la copie et de la copie de
/// sauvegarde. Ils restent choisis d'une source à l'autre, jusqu'au
/// lancement de l'import.
#[derive(Debug, Default)]
pub struct ChosenDestinations(Mutex<Chosen>);

#[derive(Debug, Default)]
struct Chosen {
    copy: Option<PathBuf>,
    backup: Option<PathBuf>,
}

impl ChosenDestinations {
    fn lock(&self) -> std::sync::MutexGuard<'_, Chosen> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Retient la destination de la copie.
    pub fn remember(&self, destination: PathBuf) {
        self.lock().copy = Some(destination);
    }

    /// Retient la destination de la copie de sauvegarde.
    pub fn remember_backup(&self, destination: PathBuf) {
        self.lock().backup = Some(destination);
    }

    /// Destination choisie pour la copie.
    #[must_use]
    pub fn current(&self) -> Option<PathBuf> {
        self.lock().copy.clone()
    }

    /// Destination choisie pour la copie de sauvegarde.
    #[must_use]
    pub fn current_backup(&self) -> Option<PathBuf> {
        self.lock().backup.clone()
    }

    /// Oublie les dossiers choisis (import lancé, écran fermé).
    pub fn clear(&self) {
        *self.lock() = Chosen::default();
    }
}

/// Dossier de la copie de sauvegarde, désigné sans chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BackupChoice {
    /// Celui du dernier import.
    Remembered,
    /// Celui choisi avec « Choisir… » dans l'écran d'import.
    Chosen,
}

/// Façon d'importer, pour l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportModeDto {
    /// Laisser les fichiers à leur place.
    Reference,
    /// Copier les fichiers, rangés par date.
    Copy,
}

impl From<ImportMode> for ImportModeDto {
    fn from(mode: ImportMode) -> Self {
        match mode {
            ImportMode::Reference => Self::Reference,
            ImportMode::Copy => Self::Copy,
        }
    }
}

/// Destination d'une copie, désignée sans chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DestinationChoice {
    /// Celle du dernier import par copie (à défaut, celle par défaut).
    Remembered,
    /// Le dossier `Media` du dossier du catalogue.
    Default,
    /// Celle choisie avec « Choisir… » dans l'écran d'import.
    Chosen,
}

/// Choix présélectionnés dans la fenêtre d'import.
// Autant de cases à cocher indépendantes de l'écran d'import.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreferencesDto {
    /// Laisser ou Copier.
    pub mode: ImportModeDto,
    /// Destination retenue (affichée).
    pub destination: String,
    /// Destination par défaut (affichée).
    pub default_destination: String,
    /// Ajouter le chemin d'origine à la légende.
    pub original_path_in_caption: bool,
    /// Empiler les paires RAW + JPEG.
    pub stack_pairs: bool,
    /// Empiler les rafales.
    pub stack_bursts: bool,
    /// Écart maximal entre deux photos d'une rafale, en secondes.
    pub burst_gap_seconds: u32,
    /// Écarts proposés, en secondes.
    pub burst_gap_choices: Vec<u32>,
    /// Rangement des copies.
    pub layout: DestinationLayout,
    /// Modèle de renommage des copies.
    pub rename_template: Option<RenameTemplate>,
    /// Dossier de la copie de sauvegarde (affiché).
    pub backup_destination: Option<String>,
    /// Lire aussi les sous-dossiers de la source.
    pub include_subfolders: bool,
}

/// Raison pour laquelle les choix n'ont pas pu être lus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportPreferencesFailure {
    /// Aucun catalogue n'est ouvert, ou il est illisible.
    CatalogUnavailable,
}

/// Lit les choix du dernier import, pour les présélectionner.
///
/// # Errors
///
/// [`ImportPreferencesFailure::CatalogUnavailable`] sans catalogue lisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_import_preferences(
    app: AppHandle,
) -> Result<ImportPreferencesDto, ImportPreferencesFailure> {
    let default_destination =
        default_destination(&app).ok_or(ImportPreferencesFailure::CatalogUnavailable)?;
    let preferences = read_preferences(&app).ok_or(ImportPreferencesFailure::CatalogUnavailable)?;
    Ok(ImportPreferencesDto {
        mode: preferences.mode.into(),
        destination: import_preferences::destination_or(&preferences, &default_destination)
            .display()
            .to_string(),
        default_destination: default_destination.display().to_string(),
        original_path_in_caption: preferences.original_path_in_caption,
        stack_pairs: preferences.stacking.pairs,
        stack_bursts: preferences.stacking.bursts,
        burst_gap_seconds: preferences.stacking.burst_gap_seconds,
        burst_gap_choices: crate::stacks::BURST_GAP_CHOICES.to_vec(),
        rename_template: preferences.rename.clone(),
        backup_destination: preferences
            .backup_destination
            .as_ref()
            .map(|path| path.display().to_string()),
        include_subfolders: preferences.include_subfolders,
        layout: preferences.layout,
    })
}

/// Raison pour laquelle la destination n'a pas pu être choisie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChooseDestinationFailure {
    /// La fenêtre de sélection a renvoyé un emplacement inutilisable.
    UnsupportedLocation,
}

/// Fait choisir le dossier où copier les fichiers. Renvoie le dossier choisi (pour l'afficher), ou `None`
/// si l'utilisateur ferme la fenêtre.
///
/// # Errors
///
/// [`ChooseDestinationFailure::UnsupportedLocation`] si l'emplacement
/// choisi n'est pas un chemin.
#[tauri::command]
#[specta::specta]
pub async fn choose_import_destination(
    app: AppHandle,
    chosen: State<'_, ChosenDestinations>,
    dialog_title: String,
) -> Result<Option<String>, ChooseDestinationFailure> {
    let start_dir = destination(&app, DestinationChoice::Remembered, None)
        .filter(|dir| dir.is_dir())
        .or_else(|| {
            app.try_state::<Workspace>()
                .map(|workspace| workspace.root().to_path_buf())
        });
    let Some(folder) = dialogs::pick_folder(&app, dialog_title, start_dir)
        .map_err(|_| ChooseDestinationFailure::UnsupportedLocation)?
    else {
        return Ok(None);
    };
    let shown = folder.display().to_string();
    chosen.remember(folder);
    Ok(Some(shown))
}

/// Fait choisir le dossier de la copie de sauvegarde. Renvoie le dossier
/// choisi (pour l'afficher), ou `None` si l'utilisateur ferme la fenêtre.
///
/// # Errors
///
/// [`ChooseDestinationFailure::UnsupportedLocation`] si l'emplacement
/// choisi n'est pas un chemin.
#[tauri::command]
#[specta::specta]
pub async fn choose_backup_destination(
    app: AppHandle,
    chosen: State<'_, ChosenDestinations>,
    dialog_title: String,
) -> Result<Option<String>, ChooseDestinationFailure> {
    let start_dir = read_preferences(&app)
        .and_then(|preferences| preferences.backup_destination)
        .filter(|dir| dir.is_dir());
    let Some(folder) = dialogs::pick_folder(&app, dialog_title, start_dir)
        .map_err(|_| ChooseDestinationFailure::UnsupportedLocation)?
    else {
        return Ok(None);
    };
    let shown = folder.display().to_string();
    chosen.remember_backup(folder);
    Ok(Some(shown))
}

/// Dossier de sauvegarde désigné par `choice` ; `chosen` : celui choisi
/// avec « Choisir… ».
#[must_use]
pub fn backup_destination(
    app: &AppHandle,
    choice: BackupChoice,
    chosen: Option<PathBuf>,
) -> Option<PathBuf> {
    match choice {
        BackupChoice::Chosen => chosen,
        BackupChoice::Remembered => read_preferences(app)?.backup_destination,
    }
}

/// Dossier désigné par `choice` ; `chosen` : celui choisi avec « Choisir… ».
/// `None` sans catalogue ouvert, ou sans dossier choisi.
#[must_use]
pub fn destination(
    app: &AppHandle,
    choice: DestinationChoice,
    chosen: Option<PathBuf>,
) -> Option<PathBuf> {
    match choice {
        DestinationChoice::Chosen => chosen,
        DestinationChoice::Default => default_destination(app),
        DestinationChoice::Remembered => {
            let default_destination = default_destination(app)?;
            let preferences = read_preferences(app)?;
            Some(import_preferences::destination_or(
                &preferences,
                &default_destination,
            ))
        }
    }
}

/// Réglages d'une copie retenus pour l'import suivant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RememberedCopy {
    /// Destination.
    pub destination: PathBuf,
    /// Chemin d'origine dans la légende.
    pub original_path_in_caption: bool,
    /// Rangement.
    pub layout: DestinationLayout,
    /// Modèle de renommage.
    pub rename: Option<RenameTemplate>,
    /// Dossier de la copie de sauvegarde.
    pub backup: Option<PathBuf>,
}

/// Retient les choix d'un import pour le suivant (dont l'empilement des
/// paires RAW + JPEG et des rafales). Une copie vers le dossier
/// par défaut est retenue comme « par défaut » : elle suit le catalogue
/// s'il est déplacé. Un échec est seulement noté au journal : l'import a
/// lieu quand même.
pub fn remember_choices(
    app: &AppHandle,
    mode: ImportMode,
    copy: Option<RememberedCopy>,
    stacking: ImportStacking,
) {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return;
    };
    let default_destination = default_destination(app);
    let catalog = lock(&catalog);
    let connection = catalog.connection();
    let result = import_preferences::read(connection).and_then(|mut preferences| {
        preferences.mode = mode;
        preferences.stacking = stacking;
        if let Some(copy) = copy {
            preferences.destination = (Some(&copy.destination) != default_destination.as_ref())
                .then_some(copy.destination);
            preferences.original_path_in_caption = copy.original_path_in_caption;
            preferences.layout = copy.layout;
            preferences.rename = copy.rename;
            preferences.backup_destination = copy.backup;
        }
        import_preferences::write(connection, &preferences)
    });
    if let Err(error) = result {
        tracing::warn!(%error, "cannot remember the import choices");
    }
}

/// Retient le choix d'inclure les sous-dossiers de la source. Un échec est seulement noté au journal.
pub fn remember_include_subfolders(app: &AppHandle, include_subfolders: bool) {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return;
    };
    let catalog = lock(&catalog);
    let connection = catalog.connection();
    let result = import_preferences::read(connection).and_then(|mut preferences| {
        preferences.include_subfolders = include_subfolders;
        import_preferences::write(connection, &preferences)
    });
    if let Err(error) = result {
        tracing::warn!(%error, "cannot remember the subfolder choice");
    }
}

fn default_destination(app: &AppHandle) -> Option<PathBuf> {
    app.try_state::<Workspace>()
        .map(|workspace| workspace.media_dir())
}

fn read_preferences(app: &AppHandle) -> Option<ImportPreferences> {
    let catalog = app.try_state::<Mutex<Catalog>>()?;
    let catalog = lock(&catalog);
    import_preferences::read(catalog.connection())
        .inspect_err(|error| tracing::error!(%error, "cannot read the import choices"))
        .ok()
}
