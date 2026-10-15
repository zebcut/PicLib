//! Mise à la corbeille avec le gestionnaire de fichiers de macOS, comme le
//! Finder : le fichier reste récupérable (« Remettre ») tant que la corbeille
//! n'est pas vidée.

use std::path::{Path, PathBuf};

use objc2_foundation::{NSFileManager, NSURL};

use crate::platform::TrashError;

/// Place un fichier dans la corbeille et renvoie son nouvel emplacement,
/// s'il est connu.
pub fn move_to_trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
    let url = NSURL::from_file_path(path).ok_or(TrashError::InvalidPath)?;
    let mut resulting = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
        .map_err(|error| TrashError::System(error.localizedDescription().to_string()))?;
    Ok(resulting.and_then(|url| url.to_file_path()))
}
