//! Préférences de l'utilisateur (fenêtre Préférences) : cache des aperçus,
//! sauvegarde automatique du catalogue.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::catalog::auto_backup::{BackupFrequency, automatic_backups};
use crate::settings::{
    self, BACKUPS_TO_KEEP_CHOICES, BYTES_PER_MB, PREVIEW_CACHE_LIMITS_MB, Preferences,
};
use crate::thumbnails::PreviewCache;
use crate::workspace::Workspace;

/// Préférences en vigueur et emplacement du fichier où elles sont
/// enregistrées.
#[derive(Debug)]
pub struct AppPreferences {
    /// Fichier de réglages ; absent si le système n'a pas fourni son dossier
    /// de configuration (les préférences ne peuvent alors pas être changées).
    settings_path: Option<PathBuf>,
    current: Mutex<Preferences>,
}

impl AppPreferences {
    /// Lit les préférences enregistrées (celles par défaut en cas de
    /// problème, qui est journalisé).
    pub fn load_for_app<R: Runtime>(app: &AppHandle<R>) -> Self {
        let settings_path = app
            .path()
            .app_config_dir()
            .inspect_err(|error| tracing::error!(%error, "cannot resolve the configuration folder"))
            .ok()
            .map(|config_dir| settings::settings_file(&config_dir));
        let current = settings_path
            .as_deref()
            .map(settings::load_preferences)
            .unwrap_or_default();
        Self {
            settings_path,
            current: Mutex::new(current),
        }
    }

    /// Préférences en vigueur.
    #[must_use]
    pub fn current(&self) -> Preferences {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Préférences et informations affichées dans la fenêtre Préférences.
// Chaque préférence oui/non est un champ, réglé à part : c'est voulu.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesView {
    /// Cache des aperçus de la Loupe.
    pub preview_cache: PreviewCacheView,
    /// Sauvegarde automatique du catalogue.
    pub catalog_backup: CatalogBackupView,
    /// Mémoriser un filtre pour chaque dossier ou mot-clé.
    pub remember_filter_per_source: bool,
    /// Note, marqueur ou couleur d'une pile repliée : à toute la pile.
    pub triage_whole_stack: bool,
    /// Ouvrir l'écran d'import quand une carte mémoire est branchée.
    pub open_import_on_card: bool,
    /// Écrire les métadonnées dans les fichiers XMP après chaque modification.
    pub write_xmp_automatically: bool,
}

/// Fréquence des sauvegardes automatiques, telle qu'échangée avec
/// l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BackupFrequencyDto {
    /// À chaque fermeture.
    EveryClose,
    /// Une fois par jour.
    Daily,
    /// Une fois par semaine.
    Weekly,
}

/// Réglages et état de la sauvegarde automatique du catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBackupView {
    /// Fréquence.
    pub frequency: BackupFrequencyDto,
    /// Nombre de sauvegardes gardées.
    pub keep: u32,
    /// Nombres proposés.
    pub keep_choices: Vec<u32>,
    /// Jour de la dernière sauvegarde automatique (`2026-09-24`), s'il y en
    /// a une.
    pub latest: Option<String>,
}

/// Réglage et état du cache des aperçus.
// Le suffixe commun donne l'unité de chaque champ : il est voulu.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCacheView {
    /// Place maximale, en mégaoctets.
    pub limit_mb: u32,
    /// Valeurs proposées pour la place maximale, en mégaoctets.
    pub limit_choices_mb: Vec<u32>,
    /// Place occupée actuellement, en mégaoctets (arrondie au supérieur) ;
    /// absente si aucun catalogue n'est ouvert.
    pub used_mb: Option<u32>,
}

/// Nouvelles préférences choisies par l'utilisateur.
// Chaque préférence oui/non est un champ, réglé à part : c'est voulu.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesInput {
    /// Place maximale du cache des aperçus, en mégaoctets.
    pub preview_cache_limit_mb: u32,
    /// Fréquence des sauvegardes automatiques.
    pub backup_frequency: BackupFrequencyDto,
    /// Nombre de sauvegardes automatiques gardées.
    pub backups_to_keep: u32,
    /// Mémoriser un filtre pour chaque dossier ou mot-clé.
    pub remember_filter_per_source: bool,
    /// Note, marqueur ou couleur d'une pile repliée : à toute la pile.
    pub triage_whole_stack: bool,
    /// Ouvrir l'écran d'import quand une carte mémoire est branchée.
    pub open_import_on_card: bool,
    /// Écrire les métadonnées dans les fichiers XMP après chaque modification.
    pub write_xmp_automatically: bool,
}

/// Raison pour laquelle les préférences n'ont pas été enregistrées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreferencesFailure {
    /// Une valeur ne fait pas partie des choix proposés.
    InvalidValue,
    /// Le fichier de réglages n'a pas pu être écrit.
    SaveFailed,
}

/// Renvoie les préférences en vigueur et la place occupée par les aperçus.
// Tauri transmet les états par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn get_preferences(app: AppHandle) -> PreferencesView {
    // Le dossier des sauvegardes et le cache des aperçus sont mesurés sur le
    // disque : hors du fil principal.
    let current = app.state::<AppPreferences>().current();
    let latest_backup = app.try_state::<Workspace>().and_then(|workspace| {
        automatic_backups(&workspace.backups_dir())
            .last()
            .map(|backup| backup.date.to_string())
    });
    PreferencesView {
        catalog_backup: CatalogBackupView {
            frequency: current.backup_frequency.into(),
            keep: current.backups_to_keep,
            keep_choices: BACKUPS_TO_KEEP_CHOICES.to_vec(),
            latest: latest_backup,
        },
        remember_filter_per_source: current.remember_filter_per_source,
        triage_whole_stack: current.triage_whole_stack,
        open_import_on_card: current.open_import_on_card,
        write_xmp_automatically: current.write_xmp_automatically,
        preview_cache: PreviewCacheView {
            limit_mb: current.preview_cache_limit_mb,
            limit_choices_mb: PREVIEW_CACHE_LIMITS_MB.to_vec(),
            used_mb: app
                .try_state::<PreviewCache>()
                .map(|previews| megabytes(previews.used_bytes())),
        },
    }
}

/// Enregistre de nouvelles préférences et les applique aussitôt (une limite
/// plus basse efface les aperçus en trop).
///
/// # Errors
///
/// Voir [`PreferencesFailure`].
// Tâche asynchrone : effacer des aperçus ne doit pas figer l'interface.
#[tauri::command]
#[specta::specta]
pub async fn set_preferences(
    app: AppHandle,
    preferences: State<'_, AppPreferences>,
    input: PreferencesInput,
) -> Result<(), PreferencesFailure> {
    let chosen = Preferences {
        preview_cache_limit_mb: input.preview_cache_limit_mb,
        backup_frequency: input.backup_frequency.into(),
        backups_to_keep: input.backups_to_keep,
        remember_filter_per_source: input.remember_filter_per_source,
        triage_whole_stack: input.triage_whole_stack,
        open_import_on_card: input.open_import_on_card,
        write_xmp_automatically: input.write_xmp_automatically,
    };
    if !chosen.is_valid() {
        return Err(PreferencesFailure::InvalidValue);
    }
    let Some(settings_path) = preferences.settings_path.as_deref() else {
        return Err(PreferencesFailure::SaveFailed);
    };
    settings::save_preferences(settings_path, chosen).map_err(|error| {
        tracing::error!(%error, "cannot save the preferences");
        PreferencesFailure::SaveFailed
    })?;
    *preferences
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = chosen;
    tracing::info!(?chosen, "preferences saved");
    if let Some(previews) = app.try_state::<PreviewCache>() {
        previews.set_limit(chosen.preview_cache_limit_bytes());
    }
    Ok(())
}

impl From<BackupFrequency> for BackupFrequencyDto {
    fn from(frequency: BackupFrequency) -> Self {
        match frequency {
            BackupFrequency::EveryClose => Self::EveryClose,
            BackupFrequency::Daily => Self::Daily,
            BackupFrequency::Weekly => Self::Weekly,
        }
    }
}

impl From<BackupFrequencyDto> for BackupFrequency {
    fn from(frequency: BackupFrequencyDto) -> Self {
        match frequency {
            BackupFrequencyDto::EveryClose => Self::EveryClose,
            BackupFrequencyDto::Daily => Self::Daily,
            BackupFrequencyDto::Weekly => Self::Weekly,
        }
    }
}

/// Octets convertis en mégaoctets, arrondis au supérieur.
fn megabytes(bytes: u64) -> u32 {
    u32::try_from(bytes.div_ceil(BYTES_PER_MB)).unwrap_or(u32::MAX)
}
