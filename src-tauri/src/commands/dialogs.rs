//! Fenêtres système de sélection, ouvertes par le cœur : l'interface ne
//! fournit que le titre traduit, jamais de chemin. Seule exception, la
//! relocalisation, qui montre l'emplacement choisi avant de l'appliquer :
//! l'interface le renvoie, et [`checked_path`] le vérifie.

use std::path::{Component, Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// La fenêtre de sélection a renvoyé un emplacement qui n'est pas un chemin
/// du disque (cas des plateformes mobiles, non prises en charge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedLocation;

/// Chemin renvoyé par l'interface (un emplacement qu'une fenêtre de
/// sélection lui a donné) : accepté seulement s'il est absolu, sans « . »
/// ni « .. » et sans caractère nul. L'interface ne peut ainsi désigner
/// qu'un emplacement réel, jamais un chemin relatif ou détourné. Un « . »
/// au milieu (sans effet) est retiré.
///
/// # Errors
///
/// [`UnsupportedLocation`] si le chemin ne remplit pas ces conditions.
pub fn checked_path(text: &str) -> Result<PathBuf, UnsupportedLocation> {
    let path = Path::new(text);
    let is_plain = path.components().all(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::Normal(_)
        )
    });
    if text.contains('\0') || !path.is_absolute() || !is_plain {
        return Err(UnsupportedLocation);
    }
    // Rebâti morceau par morceau : « /a/./b » devient « /a/b ».
    Ok(path.components().collect())
}

/// Fait choisir un dossier existant. Renvoie `None` si l'utilisateur ferme la
/// fenêtre sans choisir.
///
/// Bloque jusqu'au choix : à n'appeler que depuis une commande asynchrone,
/// jamais depuis le fil principal de l'application.
///
/// # Errors
///
/// [`UnsupportedLocation`] si l'emplacement choisi n'est pas un chemin.
pub fn pick_folder(
    app: &AppHandle,
    title: String,
    start_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>, UnsupportedLocation> {
    let mut dialog = app.dialog().file().set_title(title);
    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .blocking_pick_folder()
        .map(|chosen| chosen.into_path().map_err(|_| UnsupportedLocation))
        .transpose()
}

/// Fait choisir un fichier texte (`.txt`) à lire. Renvoie `None` si
/// l'utilisateur ferme la fenêtre sans choisir.
///
/// Bloque jusqu'au choix : à n'appeler que depuis une commande asynchrone.
///
/// # Errors
///
/// [`UnsupportedLocation`] si l'emplacement choisi n'est pas un chemin.
pub fn pick_text_file(
    app: &AppHandle,
    title: String,
    filter_name: &str,
) -> Result<Option<PathBuf>, UnsupportedLocation> {
    app.dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, &["txt"])
        .blocking_pick_file()
        .map(|chosen| chosen.into_path().map_err(|_| UnsupportedLocation))
        .transpose()
}

/// Fait choisir où enregistrer un fichier texte (`.txt`), en proposant
/// `default_name`. Le système demande lui-même confirmation avant de
/// remplacer un fichier existant. Renvoie `None` si l'utilisateur ferme la
/// fenêtre sans choisir.
///
/// Bloque jusqu'au choix : à n'appeler que depuis une commande asynchrone.
///
/// # Errors
///
/// [`UnsupportedLocation`] si l'emplacement choisi n'est pas un chemin.
pub fn pick_text_save_path(
    app: &AppHandle,
    title: String,
    filter_name: &str,
    default_name: String,
) -> Result<Option<PathBuf>, UnsupportedLocation> {
    app.dialog()
        .file()
        .set_title(title)
        .set_file_name(default_name)
        .add_filter(filter_name, &["txt"])
        .blocking_save_file()
        .map(|chosen| chosen.into_path().map_err(|_| UnsupportedLocation))
        .transpose()
}

/// Extensions des photos et vidéos prises en charge, pour la fenêtre de
/// sélection d'un fichier.
const MEDIA_EXTENSIONS: [&str; 21] = [
    "jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "psd", "psb", "cr2", "cr3", "nef", "arw",
    "raf", "dng", "mp4", "mov", "m4v", "avi", "mts", "mkv",
];

/// Fait choisir une photo ou une vidéo. Renvoie `None` si l'utilisateur
/// ferme la fenêtre sans choisir.
///
/// Bloque jusqu'au choix : à n'appeler que depuis une commande asynchrone.
///
/// # Errors
///
/// [`UnsupportedLocation`] si l'emplacement choisi n'est pas un chemin.
pub fn pick_media_file(
    app: &AppHandle,
    title: String,
    filter_name: &str,
    start_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>, UnsupportedLocation> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, &MEDIA_EXTENSIONS);
    if let Some(dir) = start_dir {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .blocking_pick_file()
        .map(|chosen| chosen.into_path().map_err(|_| UnsupportedLocation))
        .transpose()
}
