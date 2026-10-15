//! Préréglages d'import : tous les réglages de
//! l'écran d'import enregistrés sous un nom (« Carte Sony »), puis repris
//! d'un clic. Les dossiers restent dans le cœur : l'interface les désigne
//! sans chemin et les affiche seulement.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::edits::lock;
use super::import_destination::{
    self, BackupChoice, ChosenDestinations, DestinationChoice, ImportModeDto,
};
use crate::catalog::Catalog;
use crate::catalog::repositories::import_presets;
use crate::import::DestinationLayout;
use crate::renaming::RenameTemplate;
use crate::workspace::Workspace;

/// Contenu d'un préréglage, tel qu'il est enregistré (JSON).
// Autant de cases à cocher indépendantes de l'écran d'import.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetContent {
    mode: ImportModeDto,
    /// `None` : le dossier `Media` du catalogue.
    destination: Option<PathBuf>,
    original_path_in_caption: bool,
    layout: DestinationLayout,
    rename: Option<RenameTemplate>,
    /// `None` : pas de copie de sauvegarde.
    backup: Option<PathBuf>,
    skip_duplicates: bool,
    stack_pairs: bool,
    stack_bursts: bool,
    burst_gap_seconds: u32,
    template_id: Option<u32>,
    /// Absent d'un préréglage enregistré par une version antérieure : oui.
    #[serde(default = "include_subfolders_by_default")]
    include_subfolders: bool,
}

fn include_subfolders_by_default() -> bool {
    true
}

/// Préréglage proposé dans l'écran d'import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPresetDto {
    /// Identifiant.
    pub id: u32,
    /// Nom.
    pub name: String,
}

/// Réglages à enregistrer sous un nom.
// Autant de cases à cocher indépendantes de l'écran d'import.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPresetInput {
    /// Laisser ou Copier.
    pub mode: ImportModeDto,
    /// Destination d'une copie.
    pub destination: DestinationChoice,
    /// Chemin d'origine dans la légende.
    pub original_path_in_caption: bool,
    /// Rangement des copies.
    pub layout: DestinationLayout,
    /// Modèle de renommage.
    pub rename: Option<RenameTemplate>,
    /// Copie de sauvegarde, s'il y en a une.
    pub backup: Option<BackupChoice>,
    /// Ne pas importer les doublons.
    pub skip_duplicates: bool,
    /// Empiler les paires RAW + JPEG.
    pub stack_pairs: bool,
    /// Empiler les rafales.
    pub stack_bursts: bool,
    /// Écart maximal entre deux photos d'une rafale, en secondes.
    pub burst_gap_seconds: u32,
    /// Modèle de métadonnées.
    pub template_id: Option<u32>,
    /// Lire aussi les sous-dossiers de la source.
    pub include_subfolders: bool,
}

/// Réglages d'un préréglage repris : les dossiers sont « choisis » dans
/// l'écran d'import (ou celui par défaut), et affichés.
// Autant de cases à cocher indépendantes de l'écran d'import.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPresetSettingsDto {
    /// Laisser ou Copier.
    pub mode: ImportModeDto,
    /// Destination, désignée sans chemin.
    pub destination: DestinationChoice,
    /// Destination affichée.
    pub shown_destination: String,
    /// Chemin d'origine dans la légende.
    pub original_path_in_caption: bool,
    /// Rangement des copies.
    pub layout: DestinationLayout,
    /// Modèle de renommage.
    pub rename: Option<RenameTemplate>,
    /// Dossier de sauvegarde affiché (`None` : pas de sauvegarde) ; il est
    /// ensuite désigné par « choisi ».
    pub shown_backup: Option<String>,
    /// Ne pas importer les doublons.
    pub skip_duplicates: bool,
    /// Empiler les paires RAW + JPEG.
    pub stack_pairs: bool,
    /// Empiler les rafales.
    pub stack_bursts: bool,
    /// Écart maximal entre deux photos d'une rafale, en secondes.
    pub burst_gap_seconds: u32,
    /// Modèle de métadonnées.
    pub template_id: Option<u32>,
    /// Lire aussi les sous-dossiers de la source.
    pub include_subfolders: bool,
}

/// Raison pour laquelle un préréglage n'a pas pu être lu ou écrit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportPresetFailure {
    /// Aucun catalogue n'est ouvert, ou il est illisible.
    CatalogUnavailable,
    /// Le nom est vide.
    EmptyName,
    /// Ce préréglage n'existe plus.
    NotFound,
    /// Il est illisible (écrit par une version plus récente).
    Unreadable,
    /// Un dossier désigné n'est pas choisi.
    DestinationUnavailable,
}

fn unavailable(error: &dyn std::fmt::Display) -> ImportPresetFailure {
    tracing::error!(%error, "cannot reach the import presets");
    ImportPresetFailure::CatalogUnavailable
}

fn catalog(app: &AppHandle) -> Result<tauri::State<'_, Mutex<Catalog>>, ImportPresetFailure> {
    app.try_state::<Mutex<Catalog>>()
        .ok_or(ImportPresetFailure::CatalogUnavailable)
}

/// Préréglages enregistrés, par ordre alphabétique.
///
/// # Errors
///
/// [`ImportPresetFailure::CatalogUnavailable`] sans catalogue lisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn get_import_presets(
    app: AppHandle,
) -> Result<Vec<ImportPresetDto>, ImportPresetFailure> {
    let catalog = catalog(&app)?;
    let presets =
        import_presets::list(lock(&catalog).connection()).map_err(|error| unavailable(&error))?;
    Ok(presets
        .into_iter()
        .map(|preset| ImportPresetDto {
            id: u32::try_from(preset.id).unwrap_or(u32::MAX),
            name: preset.name,
        })
        .collect())
}

/// Enregistre les réglages sous `name` ; un préréglage du même nom est
/// remplacé.
///
/// # Errors
///
/// Nom vide, dossier désigné inconnu, catalogue illisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn save_import_preset(
    app: AppHandle,
    chosen: State<'_, ChosenDestinations>,
    name: String,
    settings: ImportPresetInput,
) -> Result<(), ImportPresetFailure> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ImportPresetFailure::EmptyName);
    }
    let destination = match settings.destination {
        DestinationChoice::Default => None,
        choice => Some(
            import_destination::destination(&app, choice, chosen.current())
                .ok_or(ImportPresetFailure::DestinationUnavailable)?,
        ),
    };
    // Le dossier par défaut suit le catalogue s'il est déplacé.
    let default = app
        .try_state::<Workspace>()
        .map(|workspace| workspace.media_dir());
    let destination = destination.filter(|path| Some(path) != default.as_ref());
    let backup = match settings.backup {
        Some(choice) => Some(
            import_destination::backup_destination(&app, choice, chosen.current_backup())
                .ok_or(ImportPresetFailure::DestinationUnavailable)?,
        ),
        None => None,
    };
    let content = PresetContent {
        mode: settings.mode,
        destination,
        original_path_in_caption: settings.original_path_in_caption,
        layout: settings.layout,
        rename: settings.rename,
        backup,
        skip_duplicates: settings.skip_duplicates,
        stack_pairs: settings.stack_pairs,
        stack_bursts: settings.stack_bursts,
        burst_gap_seconds: settings.burst_gap_seconds,
        template_id: settings.template_id,
        include_subfolders: settings.include_subfolders,
    };
    let text = serde_json::to_string(&content).map_err(|error| unavailable(&error))?;
    let catalog = catalog(&app)?;
    import_presets::put(lock(&catalog).connection(), name, &text)
        .map_err(|error| unavailable(&error))
}

/// Reprend un préréglage : ses dossiers deviennent les dossiers choisis de
/// l'écran d'import, et ses réglages sont renvoyés pour être affichés.
///
/// # Errors
///
/// Préréglage inconnu ou illisible, catalogue illisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn apply_import_preset(
    app: AppHandle,
    chosen: State<'_, ChosenDestinations>,
    id: u32,
) -> Result<ImportPresetSettingsDto, ImportPresetFailure> {
    let stored = {
        let catalog = catalog(&app)?;
        import_presets::find(lock(&catalog).connection(), i64::from(id))
            .map_err(|error| unavailable(&error))?
            .ok_or(ImportPresetFailure::NotFound)?
    };
    let content: PresetContent = serde_json::from_str(&stored.content).map_err(|error| {
        tracing::warn!(%error, "unreadable import preset");
        ImportPresetFailure::Unreadable
    })?;
    let (destination, shown_destination) = match content.destination {
        Some(path) => {
            let shown = path.display().to_string();
            chosen.remember(path);
            (DestinationChoice::Chosen, shown)
        }
        None => (
            DestinationChoice::Default,
            app.try_state::<Workspace>()
                .map(|workspace| workspace.media_dir().display().to_string())
                .unwrap_or_default(),
        ),
    };
    let shown_backup = content.backup.map(|path| {
        let shown = path.display().to_string();
        chosen.remember_backup(path);
        shown
    });
    Ok(ImportPresetSettingsDto {
        mode: content.mode,
        destination,
        shown_destination,
        original_path_in_caption: content.original_path_in_caption,
        layout: content.layout,
        rename: content.rename,
        shown_backup,
        skip_duplicates: content.skip_duplicates,
        stack_pairs: content.stack_pairs,
        stack_bursts: content.stack_bursts,
        burst_gap_seconds: content.burst_gap_seconds,
        template_id: content.template_id,
        include_subfolders: content.include_subfolders,
    })
}

/// Supprime un préréglage.
///
/// # Errors
///
/// [`ImportPresetFailure::CatalogUnavailable`] sans catalogue lisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn delete_import_preset(app: AppHandle, id: u32) -> Result<(), ImportPresetFailure> {
    let catalog = catalog(&app)?;
    import_presets::delete(lock(&catalog).connection(), i64::from(id))
        .map_err(|error| unavailable(&error))
}
