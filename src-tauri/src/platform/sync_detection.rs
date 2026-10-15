//! Détection des dossiers synchronisés avec un service en ligne.

use std::ffi::OsStr;
use std::path::Path;

/// Suffixe qui exclut un élément de la synchronisation iCloud Drive.
const NOSYNC_SUFFIX: &str = ".nosync";

/// Remonte de `path` vers la racine du disque : le chemin est synchronisé si
/// l'un de ses dossiers parents est la racine d'un service de synchronisation,
/// sauf si un dossier `.nosync` se trouve entre les deux.
///
/// `is_provider_root` indique si un dossier est la racine d'un service de
/// synchronisation ; il est fourni par le système (voir `macos.rs`), ce qui
/// permet de tester cette logique sans vrai service de synchronisation.
pub fn is_synced_location(path: &Path, is_provider_root: impl Fn(&Path) -> bool) -> bool {
    for ancestor in path.ancestors() {
        if ancestor.file_name().is_some_and(has_nosync_suffix) {
            return false;
        }
        if is_cloud_storage_dir(ancestor) || is_provider_root(ancestor) {
            return true;
        }
    }
    false
}

fn has_nosync_suffix(name: &OsStr) -> bool {
    name.to_string_lossy().ends_with(NOSYNC_SUFFIX)
}

/// Dossiers de macOS qui contiennent les services de synchronisation
/// (OneDrive, Dropbox, Google Drive, kDrive... et iCloud Drive lui-même).
fn is_cloud_storage_dir(path: &Path) -> bool {
    path.ends_with("Library/CloudStorage") || path.ends_with("Library/Mobile Documents")
}
