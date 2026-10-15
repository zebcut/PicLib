//! Cache des vignettes sur disque (photos et vidéos).

use std::path::{Path, PathBuf};

use crate::catalog::repositories::thumbnails::PendingThumbnail;
use crate::domain::{MediaFormat, MediaId, MediaKind};
use crate::platform::{self, ImagePurpose, ThumbnailError, ThumbnailInfo, ThumbnailRequest};

/// Taille des vignettes : plus grand côté, en pixels. Nette sur un écran
/// Retina jusqu'à des vignettes d'environ 256 points à l'écran.
pub const THUMBNAIL_SIZE: u32 = 512;

/// Qualité JPEG des vignettes (0 à 1) : bon compromis netteté / place.
pub const JPEG_QUALITY: f64 = 0.8;

/// Nombre de vignettes par sous-dossier : le Finder reste rapide, même avec
/// des dizaines de milliers de fichiers.
const FILES_PER_FOLDER: u64 = 1000;

/// Erreur du cache des vignettes.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Le dossier du cache n'a pas pu être créé (disque plein, droits...).
    #[error("cannot prepare the thumbnail folder {}: {source}", path.display())]
    Folder {
        /// Dossier concerné.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// La vignette n'a pas pu être fabriquée.
    #[error(transparent)]
    Thumbnail(#[from] ThumbnailError),
    /// La vignette fabriquée n'a pas pu être mise en place.
    #[error("cannot store the thumbnail {}: {source}", path.display())]
    Store {
        /// Vignette concernée.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
}

/// Cache des vignettes d'un dossier de travail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailCache {
    root: PathBuf,
}

impl ThumbnailCache {
    /// Cache rangé dans `root` (le dossier `Vignettes`).
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Emplacement de la vignette d'un média :
    /// `Vignettes/000/000123.jpg`, mille vignettes par sous-dossier.
    #[must_use]
    pub fn path_for(&self, media_id: MediaId) -> PathBuf {
        sharded_path(&self.root, media_id)
    }

    /// Efface la vignette d'un média qui n'est plus au catalogue (fichier
    /// placé dans la corbeille) : ce n'est qu'une copie réduite, fabriquée
    /// par PicLib.
    pub fn remove(&self, media_id: MediaId) {
        let path = self.path_for(media_id);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "cannot remove a thumbnail");
            }
        }
    }

    /// Fabrique la vignette d'un média et la range dans le cache.
    ///
    /// La vignette est d'abord écrite dans un fichier temporaire, puis mise
    /// à sa place en une seule opération : le cache ne contient jamais de
    /// vignette à moitié écrite, même en cas d'arrêt brutal. Seuls des
    /// fichiers du cache sont écrits ; la photo d'origine est seulement lue.
    ///
    /// # Errors
    ///
    /// Voir [`CacheError`].
    pub fn create(&self, pending: &PendingThumbnail) -> Result<ThumbnailInfo, CacheError> {
        render_to_file(
            &RenderRequest {
                source: &pending.path,
                format: pending.format,
                purpose: ImagePurpose::Thumbnail,
                max_pixel_size: THUMBNAIL_SIZE,
                jpeg_quality: JPEG_QUALITY,
            },
            &self.path_for(pending.media_id),
        )
    }
}

/// Emplacement d'une image d'un cache : `<root>/000/000123.jpg`, mille
/// fichiers par sous-dossier.
pub(super) fn sharded_path(root: &Path, media_id: MediaId) -> PathBuf {
    let id = media_id.get().unsigned_abs();
    root.join(format!("{:03}", id / FILES_PER_FOLDER))
        .join(format!("{id:06}.jpg"))
}

/// Image réduite à fabriquer à partir d'une photo ou d'une vidéo.
pub(super) struct RenderRequest<'a> {
    pub(super) source: &'a Path,
    pub(super) format: MediaFormat,
    pub(super) purpose: ImagePurpose,
    pub(super) max_pixel_size: u32,
    pub(super) jpeg_quality: f64,
}

/// Fabrique l'image réduite d'une photo ou d'une vidéo et la range à
/// `target`.
///
/// L'image est d'abord écrite dans un fichier temporaire, puis mise à sa
/// place en une seule opération : le cache ne contient jamais d'image à
/// moitié écrite, même en cas d'arrêt brutal. Seuls des fichiers du cache
/// sont écrits ; le fichier d'origine est seulement lu.
pub(super) fn render_to_file(
    request: &RenderRequest<'_>,
    target: &Path,
) -> Result<ThumbnailInfo, CacheError> {
    let folder = target.parent().unwrap_or(target);
    std::fs::create_dir_all(folder).map_err(|source| CacheError::Folder {
        path: folder.to_path_buf(),
        source,
    })?;

    let temporary = target.with_extension("jpg.tmp");
    remove_leftover(&temporary)?;
    let platform_request = ThumbnailRequest {
        source: request.source,
        destination: &temporary,
        max_pixel_size: request.max_pixel_size,
        jpeg_quality: request.jpeg_quality,
        prefer_embedded_preview: request.format.is_raw(),
        engine: platform::preferred_engine(request.format, request.purpose),
    };
    let made = match request.format.kind() {
        MediaKind::Photo => platform::make_thumbnail(&platform_request),
        MediaKind::Video => platform::make_video_thumbnail(&platform_request),
    };
    let info = match made {
        Ok(info) => info,
        Err(error) => {
            remove_leftover(&temporary)?;
            return Err(error.into());
        }
    };
    std::fs::rename(&temporary, target).map_err(|source| CacheError::Store {
        path: target.to_path_buf(),
        source,
    })?;
    Ok(info)
}

/// Supprime un fichier temporaire du cache laissé par un arrêt brutal ou un
/// échec. Il n'appartient qu'au cache : ce n'est jamais un fichier de
/// l'utilisateur.
fn remove_leftover(temporary: &Path) -> Result<(), CacheError> {
    match std::fs::remove_file(temporary) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CacheError::Store {
            path: temporary.to_path_buf(),
            source,
        }),
    }
}
