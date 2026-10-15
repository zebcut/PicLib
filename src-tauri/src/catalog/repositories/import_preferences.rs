//! Derniers choix de la fenêtre d'import (Laisser ou Copier, destination,
//! chemin d'origine dans la légende, paires RAW + JPEG et rafales
//! empilées, rangement des copies), repris à l'import suivant.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{import_mode_from_text, import_mode_to_text, path_to_text, text_to_path};
use crate::domain::ImportMode;
use crate::import::DestinationLayout;
use crate::renaming::RenameTemplate;

/// Choix retenus pour l'import suivant.
// Autant de cases à cocher indépendantes de l'écran d'import.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreferences {
    /// Laisser les fichiers à leur place, ou les copier.
    pub mode: ImportMode,
    /// Dossier de destination d'une copie ; `None` : le dossier `Media` du
    /// dossier du catalogue.
    pub destination: Option<PathBuf>,
    /// Ajouter le chemin d'origine de chaque fichier copié à sa légende.
    pub original_path_in_caption: bool,
    /// Empilement des photos importées.
    pub stacking: ImportStacking,
    /// Rangement des copies.
    pub layout: DestinationLayout,
    /// Renommer les copies d'après ce modèle.
    pub rename: Option<RenameTemplate>,
    /// Dossier de la copie de sauvegarde ; `None` : pas de sauvegarde.
    pub backup_destination: Option<PathBuf>,
    /// Lire aussi les sous-dossiers de la source (coché par défaut).
    pub include_subfolders: bool,
}

/// Empilement des photos importées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportStacking {
    /// Empiler les paires RAW + JPEG (coché par défaut).
    pub pairs: bool,
    /// Empiler les rafales (décoché par défaut).
    pub bursts: bool,
    /// Écart maximal entre deux photos d'une rafale, en secondes.
    pub burst_gap_seconds: u32,
}

impl Default for ImportStacking {
    fn default() -> Self {
        Self {
            pairs: true,
            bursts: false,
            burst_gap_seconds: 2,
        }
    }
}

impl Default for ImportPreferences {
    /// Avant tout import : laisser les fichiers à leur place, empiler les
    /// paires RAW + JPEG.
    fn default() -> Self {
        Self {
            mode: ImportMode::Reference,
            destination: None,
            original_path_in_caption: false,
            stacking: ImportStacking::default(),
            layout: DestinationLayout::default(),
            rename: None,
            backup_destination: None,
            include_subfolders: true,
        }
    }
}

/// Lit les choix du dernier import (ceux par défaut s'il n'y en a pas).
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn read(connection: &Connection) -> Result<ImportPreferences, CatalogError> {
    let row = connection
        .query_row(
            "SELECT mode, destination, original_path_in_caption, stack_pairs, stack_bursts,
                burst_gap_seconds, layout, rename_template, backup_destination,
                include_subfolders
             FROM import_preferences WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, bool>(2)?,
                    ImportStacking {
                        pairs: row.get(3)?,
                        bursts: row.get(4)?,
                        burst_gap_seconds: row.get(5)?,
                    },
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        mode,
        destination,
        original_path_in_caption,
        stacking,
        layout,
        rename,
        backup_destination,
        include_subfolders,
    )) = row
    else {
        return Ok(ImportPreferences::default());
    };
    Ok(ImportPreferences {
        mode: import_mode_from_text(&mode)?,
        destination: destination.map(text_to_path),
        original_path_in_caption,
        stacking,
        // Un rangement illisible (écrit par une version future) : celui par
        // défaut.
        layout: layout
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default(),
        rename: rename.and_then(|text| serde_json::from_str(&text).ok()),
        backup_destination: backup_destination.map(text_to_path),
        include_subfolders,
    })
}

/// Retient les choix d'un import pour le suivant.
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn write(connection: &Connection, preferences: &ImportPreferences) -> Result<(), CatalogError> {
    let destination = preferences
        .destination
        .as_deref()
        .map(path_to_text)
        .transpose()?;
    // Écrire ce type en JSON ne peut pas échouer ; à défaut, le rangement par
    // défaut.
    let layout = serde_json::to_string(&preferences.layout).ok();
    let rename = preferences
        .rename
        .as_ref()
        .and_then(|template| serde_json::to_string(template).ok());
    let backup_destination = preferences
        .backup_destination
        .as_deref()
        .map(path_to_text)
        .transpose()?;
    connection.execute(
        "INSERT INTO import_preferences
             (id, mode, destination, original_path_in_caption, stack_pairs, stack_bursts,
              burst_gap_seconds, layout, rename_template, backup_destination,
              include_subfolders)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (id) DO UPDATE SET mode = excluded.mode,
             destination = excluded.destination,
             original_path_in_caption = excluded.original_path_in_caption,
             stack_pairs = excluded.stack_pairs,
             stack_bursts = excluded.stack_bursts,
             burst_gap_seconds = excluded.burst_gap_seconds,
             layout = excluded.layout,
             rename_template = excluded.rename_template,
             backup_destination = excluded.backup_destination,
             include_subfolders = excluded.include_subfolders",
        params![
            import_mode_to_text(preferences.mode),
            destination,
            preferences.original_path_in_caption,
            preferences.stacking.pairs,
            preferences.stacking.bursts,
            preferences.stacking.burst_gap_seconds,
            layout,
            rename,
            backup_destination,
            preferences.include_subfolders
        ],
    )?;
    Ok(())
}

/// Destination réelle d'une copie : celle retenue, ou `default_dir`.
#[must_use]
pub fn destination_or(preferences: &ImportPreferences, default_dir: &Path) -> PathBuf {
    preferences
        .destination
        .clone()
        .unwrap_or_else(|| default_dir.to_path_buf())
}
