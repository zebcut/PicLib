//! Vignettes de l'écran d'import : fabriquées à la demande
//! à partir des fichiers de la source, qui sont seulement lus, et rangées
//! dans un cache temporaire du dossier de travail, vidé quand l'écran se
//! ferme (et au démarrage, après un arrêt brutal).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::domain::MediaFormat;
use crate::platform::ImagePurpose;

use super::cache::{RenderRequest, render_to_file};
use super::{CacheError, JPEG_QUALITY};

/// Taille des vignettes de l'écran d'import : plus petites que celles de
/// la bibliothèque, pour s'afficher vite.
pub const IMPORT_THUMBNAIL_SIZE: u32 = 320;

/// Cache des vignettes de l'écran d'import, par analyse de source.
#[derive(Debug)]
pub struct ImportPreviewCache {
    root: PathBuf,
    /// Une seule fabrication à la fois : deux demandes de la même vignette
    /// ne l'écrivent pas en même temps.
    creation: Mutex<()>,
}

impl ImportPreviewCache {
    /// Cache rangé dans `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            creation: Mutex::new(()),
        }
    }

    /// Emplacement de la vignette du fichier `index` de l'analyse
    /// `analysis` : `<root>/12/34.jpg`.
    #[must_use]
    pub fn path_for(&self, analysis: u32, index: usize) -> PathBuf {
        self.root
            .join(analysis.to_string())
            .join(format!("{index}.jpg"))
    }

    /// Vignette d'un fichier de la source, fabriquée si elle n'existe pas
    /// encore. La source est seulement lue.
    ///
    /// # Errors
    ///
    /// Voir [`CacheError`].
    pub fn get_or_create(
        &self,
        analysis: u32,
        index: usize,
        source: &Path,
        format: MediaFormat,
    ) -> Result<PathBuf, CacheError> {
        let path = self.path_for(analysis, index);
        if path.is_file() {
            return Ok(path);
        }
        let _creation = self.creation.lock().unwrap_or_else(PoisonError::into_inner);
        if path.is_file() {
            return Ok(path);
        }
        render_to_file(
            &RenderRequest {
                source,
                format,
                purpose: ImagePurpose::Thumbnail,
                max_pixel_size: IMPORT_THUMBNAIL_SIZE,
                jpeg_quality: JPEG_QUALITY,
            },
            &path,
        )?;
        Ok(path)
    }

    /// Efface les vignettes d'une analyse : ce ne sont que des copies
    /// réduites fabriquées par PicLib.
    pub fn forget(&self, analysis: u32) {
        remove_dir(&self.root.join(analysis.to_string()));
    }

    /// Efface tout le cache (au démarrage).
    pub fn clear(&self) {
        remove_dir(&self.root);
    }
}

fn remove_dir(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "cannot clear the import thumbnails");
        }
    }
}
