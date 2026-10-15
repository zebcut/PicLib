//! Réglages de l'application, enregistrés dans le dossier de configuration du
//! système (sur macOS : `~/Library/Application Support/<identifiant>/`).
//!
//! Ce fichier retient notamment où se trouve le dossier de travail : il doit
//! donc rester en dehors de celui-ci, à un emplacement toujours connu.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::auto_backup::{AutoBackupPolicy, BackupFrequency};
use crate::external_editing::EditorSettings;

/// Nom du fichier de réglages dans le dossier de configuration.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// Emplacement du fichier de réglages dans le dossier de configuration de
/// l'application.
#[must_use]
pub fn settings_file(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE_NAME)
}

/// Version du format du fichier, à augmenter si sa structure change de façon
/// incompatible.
const FORMAT_VERSION: u32 = 1;

/// Tailles proposées pour le cache des aperçus, en mégaoctets (multiples de
/// 1000, comme le Finder : 1 Go = 1000 Mo).
pub const PREVIEW_CACHE_LIMITS_MB: [u32; 6] = [500, 1000, 2000, 4000, 8000, 16_000];

/// Taille par défaut du cache des aperçus, en mégaoctets.
const DEFAULT_PREVIEW_CACHE_LIMIT_MB: u32 = 1000;

/// Octets dans un mégaoctet.
pub const BYTES_PER_MB: u64 = 1_000_000;

/// Nombres de sauvegardes automatiques proposés.
pub const BACKUPS_TO_KEEP_CHOICES: [u32; 5] = [3, 5, 10, 20, 50];

/// Nombre de sauvegardes automatiques gardées par défaut.
const DEFAULT_BACKUPS_TO_KEEP: u32 = 5;

/// Réglages de l'application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    format_version: u32,
    /// Dossier de travail (catalogue, sauvegardes, vignettes).
    pub workspace_dir: PathBuf,
    /// Préférences de l'utilisateur. Absentes des fichiers écrits avant leur
    /// apparition : les valeurs par défaut s'appliquent alors.
    #[serde(default)]
    pub preferences: Preferences,
    /// Éditeurs externes : applications ajoutées, éditeur
    /// par défaut. Absents des fichiers plus anciens.
    #[serde(default)]
    pub external_editors: EditorSettings,
}

impl Settings {
    /// Réglages pointant vers le dossier de travail `workspace_dir`, avec les
    /// préférences par défaut.
    #[must_use]
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            workspace_dir,
            preferences: Preferences::default(),
            external_editors: EditorSettings::default(),
        }
    }
}

/// Préférences réglables dans la fenêtre Préférences.
// Chaque préférence oui/non est un champ, réglé à part : c'est voulu.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// Place maximale du cache des aperçus de la Loupe, en mégaoctets (une
    /// des valeurs de [`PREVIEW_CACHE_LIMITS_MB`]).
    pub preview_cache_limit_mb: u32,
    /// Fréquence des sauvegardes automatiques du catalogue.
    pub backup_frequency: BackupFrequency,
    /// Nombre de sauvegardes automatiques gardées (une des valeurs de
    /// [`BACKUPS_TO_KEEP_CHOICES`]).
    pub backups_to_keep: u32,
    /// Chaque dossier ou mot-clé retrouve le dernier filtre qu'on y a réglé
    /// ; décoché par défaut.
    pub remember_filter_per_source: bool,
    /// Une note, un marqueur ou une couleur posé sur une pile repliée va à
    /// toute la pile ; sinon à la seule photo visible (décoché par défaut, comme Lightroom).
    pub triage_whole_stack: bool,
    /// Ouvrir l'écran d'import quand une carte mémoire est branchée (décoché par défaut).
    pub open_import_on_card: bool,
    /// Écrire les métadonnées dans les fichiers XMP quelques secondes après
    /// chaque modification ; décoché par défaut,
    /// comme Lightroom : Cmd+S les écrit sur demande.
    pub write_xmp_automatically: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            preview_cache_limit_mb: DEFAULT_PREVIEW_CACHE_LIMIT_MB,
            // Par défaut : une fois par jour.
            backup_frequency: BackupFrequency::Daily,
            backups_to_keep: DEFAULT_BACKUPS_TO_KEEP,
            remember_filter_per_source: false,
            triage_whole_stack: false,
            open_import_on_card: false,
            write_xmp_automatically: false,
        }
    }
}

impl Preferences {
    /// Vrai si toutes les valeurs font partie de celles proposées.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        PREVIEW_CACHE_LIMITS_MB.contains(&self.preview_cache_limit_mb)
            && BACKUPS_TO_KEEP_CHOICES.contains(&self.backups_to_keep)
    }

    /// Réglages de la sauvegarde automatique du catalogue.
    #[must_use]
    pub fn auto_backup_policy(&self) -> AutoBackupPolicy {
        AutoBackupPolicy {
            frequency: self.backup_frequency,
            keep: usize::try_from(self.backups_to_keep).unwrap_or(usize::MAX),
        }
    }

    /// Préférences utilisables : une valeur modifiée à la main dans le
    /// fichier et hors des choix proposés est remplacée par sa valeur par
    /// défaut.
    #[must_use]
    pub fn or_default_if_invalid(self) -> Self {
        if self.is_valid() {
            self
        } else {
            tracing::warn!(?self, "invalid preferences, default values used");
            Self::default()
        }
    }

    /// Place maximale du cache des aperçus, en octets.
    #[must_use]
    pub fn preview_cache_limit_bytes(&self) -> u64 {
        u64::from(self.preview_cache_limit_mb) * BYTES_PER_MB
    }
}

/// Erreur de lecture ou d'écriture des réglages.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Le fichier existe mais n'a pas pu être lu.
    #[error("cannot read the settings file: {0}")]
    Read(#[source] std::io::Error),
    /// Le contenu du fichier n'est pas valide.
    #[error("invalid settings file: {0}")]
    Parse(#[source] serde_json::Error),
    /// Le fichier vient d'une version de PicLib qui utilise un autre format.
    #[error("unsupported settings format version {0}")]
    UnsupportedFormat(u32),
    /// Les réglages n'ont pas pu être convertis en JSON.
    #[error("cannot serialize the settings: {0}")]
    Serialize(#[source] serde_json::Error),
    /// Le fichier n'a pas pu être écrit.
    #[error("cannot write the settings file: {0}")]
    Write(#[source] std::io::Error),
    /// Le fichier n'existe pas : il est créé au premier démarrage réussi.
    #[error("the settings file does not exist yet")]
    Missing,
}

/// Lit les réglages. Renvoie `None` si le fichier n'existe pas encore
/// (premier lancement).
///
/// # Errors
///
/// Échoue si le fichier existe mais est illisible ou invalide. Il n'est alors
/// jamais remplacé automatiquement : il contient l'emplacement du catalogue.
pub fn load(path: &Path) -> Result<Option<Settings>, SettingsError> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SettingsError::Read(error)),
    };
    let settings: Settings = serde_json::from_str(&content).map_err(SettingsError::Parse)?;
    if settings.format_version != FORMAT_VERSION {
        return Err(SettingsError::UnsupportedFormat(settings.format_version));
    }
    Ok(Some(settings))
}

/// Enregistre les réglages sans risque de fichier à moitié écrit : le contenu
/// est écrit dans un fichier temporaire, forcé sur le disque, puis mis à la
/// place de l'ancien en une seule opération.
///
/// # Errors
///
/// Échoue si le dossier ou le fichier ne peut pas être écrit.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    let content = serde_json::to_string_pretty(settings).map_err(SettingsError::Serialize)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SettingsError::Write)?;
    }

    let temporary_path = path.with_extension("json.tmp");
    let write_temporary = || -> std::io::Result<()> {
        let mut file = File::create(&temporary_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()
    };
    write_temporary().map_err(SettingsError::Write)?;
    std::fs::rename(&temporary_path, path).map_err(SettingsError::Write)
}

/// Préférences enregistrées ; celles par défaut si le fichier n'existe pas,
/// est illisible, ou contient des valeurs hors des choix proposés.
#[must_use]
pub fn load_preferences(path: &Path) -> Preferences {
    match load(path) {
        Ok(Some(settings)) => settings.preferences.or_default_if_invalid(),
        Ok(None) => Preferences::default(),
        Err(error) => {
            tracing::warn!(%error, "cannot read the preferences, default values used");
            Preferences::default()
        }
    }
}

/// Enregistre de nouvelles préférences, sans toucher au reste des réglages
/// (dossier de travail).
///
/// # Errors
///
/// Échoue si le fichier n'existe pas, est illisible, ou ne peut pas être
/// écrit.
pub fn save_preferences(path: &Path, preferences: Preferences) -> Result<(), SettingsError> {
    let mut settings = load(path)?.ok_or(SettingsError::Missing)?;
    settings.preferences = preferences;
    save(path, &settings)
}

/// Réglages des éditeurs externes enregistrés (ceux par défaut si le
/// fichier manque ou est illisible : le problème est journalisé).
#[must_use]
pub fn load_editor_settings(path: &Path) -> EditorSettings {
    match load(path) {
        Ok(settings) => settings
            .map(|settings| settings.external_editors)
            .unwrap_or_default(),
        Err(error) => {
            tracing::warn!(%error, "cannot read the external editors, defaults used");
            EditorSettings::default()
        }
    }
}

/// Enregistre les réglages des éditeurs externes, sans toucher au reste.
///
/// # Errors
///
/// Échoue si le fichier n'existe pas, est illisible, ou ne peut pas être
/// écrit.
pub fn save_editor_settings(path: &Path, editors: &EditorSettings) -> Result<(), SettingsError> {
    let mut settings = load(path)?.ok_or(SettingsError::Missing)?;
    settings.external_editors = editors.clone();
    save(path, &settings)
}
