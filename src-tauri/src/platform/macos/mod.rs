//! Fonctions propres à macOS.

pub mod av_foundation;
pub mod core_image;
pub mod image_io;
pub mod rename;
pub mod trash;
pub mod uncached;

use std::path::Path;

/// Attribut étendu posé par macOS sur la racine d'un dossier géré par un
/// service de synchronisation (« File Provider ») : iCloud « Bureau et
/// Documents », OneDrive, Dropbox...
const FILE_PROVIDER_DOMAIN_ATTRIBUTE: &str = "com.apple.file-provider-domain-id";

/// Indique si `path` est la racine d'un dossier synchronisé. Un dossier
/// inexistant ou illisible est considéré comme non synchronisé.
pub fn is_file_provider_root(path: &Path) -> bool {
    matches!(
        xattr::get(path, FILE_PROVIDER_DOMAIN_ATTRIBUTE),
        Ok(Some(_))
    )
}
