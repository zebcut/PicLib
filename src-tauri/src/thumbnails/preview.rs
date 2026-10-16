//! Cache des aperçus : images de 2560 pixels affichées par la Loupe.
//!
//! Contrairement aux vignettes, les aperçus ne sont fabriqués qu'à la
//! demande (au premier affichage d'une photo en Loupe) et leur cache est
//! limité en taille : au-delà de la limite, les aperçus regardés le moins
//! récemment sont effacés. Ce ne sont que des copies réduites fabriquées par
//! PicLib, jamais des fichiers de l'utilisateur.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use crate::domain::{MediaFormat, MediaId};
use crate::platform::ImagePurpose;

use super::CacheError;
use super::cache::{RenderRequest, render_to_file, sharded_path};

/// Taille des aperçus : plus grand côté, en pixels. Remplit un écran Retina
/// de 27 pouces en affichage « adapté ».
pub const PREVIEW_SIZE: u32 = 2560;

/// Qualité JPEG des aperçus : plus élevée que celle des vignettes, car ils
/// s'affichent en grand.
pub const PREVIEW_JPEG_QUALITY: f64 = 0.85;

/// Extension des aperçus ; les autres fichiers (temporaires) sont ignorés.
const PREVIEW_EXTENSION: &str = "jpg";

/// Cache des aperçus d'un dossier de travail, partagé entre les fils
/// d'exécution.
#[derive(Debug)]
pub struct PreviewCache {
    root: PathBuf,
    state: Mutex<State>,
    /// Une seule fabrication à la fois : deux demandes du même aperçu
    /// (affichage et préchargement) ne l'écrivent pas en même temps.
    creation: Mutex<()>,
}

#[derive(Debug)]
struct State {
    limit_bytes: u64,
    /// Place occupée, mesurée au premier besoin puis tenue à jour.
    used_bytes: Option<u64>,
}

/// Aperçu présent dans le cache.
struct CachedPreview {
    path: PathBuf,
    size: u64,
    last_used: SystemTime,
}

impl PreviewCache {
    /// Cache rangé dans `root` (le dossier `Aperçus`), limité à
    /// `limit_bytes` octets.
    #[must_use]
    pub fn new(root: PathBuf, limit_bytes: u64) -> Self {
        Self {
            root,
            state: Mutex::new(State {
                limit_bytes,
                used_bytes: None,
            }),
            creation: Mutex::new(()),
        }
    }

    /// Folder of the cache.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Emplacement de l'aperçu d'un média : `Aperçus/000/000123.jpg`.
    #[must_use]
    pub fn path_for(&self, media_id: MediaId) -> PathBuf {
        sharded_path(&self.root, media_id)
    }

    /// Aperçu d'un média, fabriqué s'il n'existe pas encore, puis noté comme
    /// « regardé à l'instant » pour être effacé en dernier.
    ///
    /// # Errors
    ///
    /// Voir [`CacheError`] : fichier d'origine introuvable ou illisible,
    /// cache impossible à écrire.
    pub fn get_or_create(
        &self,
        media_id: MediaId,
        source: &Path,
        format: MediaFormat,
    ) -> Result<PathBuf, CacheError> {
        let path = self.path_for(media_id);
        if path.is_file() {
            mark_used(&path);
            return Ok(path);
        }
        let _creation = self.creation.lock().unwrap_or_else(PoisonError::into_inner);
        // Fabriqué pendant l'attente, par une autre demande du même aperçu.
        if path.is_file() {
            return Ok(path);
        }
        render_to_file(
            &RenderRequest {
                source,
                format,
                purpose: ImagePurpose::Preview,
                max_pixel_size: PREVIEW_SIZE,
                jpeg_quality: PREVIEW_JPEG_QUALITY,
            },
            &path,
        )?;
        let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        let mut state = self.lock();
        if let Some(used) = state.used_bytes.as_mut() {
            *used += size;
        }
        self.enforce_limit(&mut state, Some(&path));
        Ok(path)
    }

    /// Change la limite du cache, en effaçant aussitôt les aperçus en trop.
    pub fn set_limit(&self, limit_bytes: u64) {
        let mut state = self.lock();
        state.limit_bytes = limit_bytes;
        self.enforce_limit(&mut state, None);
    }

    /// Efface les aperçus des médias retirés du catalogue : un aperçu se
    /// refabrique au besoin (par exemple si le retrait est annulé).
    pub fn forget(&self, media_ids: &[MediaId]) {
        let mut state = self.lock();
        for &media_id in media_ids {
            let path = self.path_for(media_id);
            let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    if let Some(used) = state.used_bytes.as_mut() {
                        *used = used.saturating_sub(size);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(%error, path = %path.display(), "cannot remove a preview");
                }
            }
        }
    }

    /// Place occupée par les aperçus, en octets.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        let mut state = self.lock();
        *state
            .used_bytes
            .get_or_insert_with(|| self.list().iter().map(|preview| preview.size).sum())
    }

    /// Efface les aperçus regardés le moins récemment tant que la limite est
    /// dépassée, sans jamais effacer `keep` (l'aperçu qu'on va afficher).
    fn enforce_limit(&self, state: &mut State, keep: Option<&Path>) {
        let used = *state
            .used_bytes
            .get_or_insert_with(|| self.list().iter().map(|preview| preview.size).sum());
        if used <= state.limit_bytes {
            return;
        }
        let mut previews = self.list();
        previews.sort_by_key(|preview| preview.last_used);
        let mut remaining: u64 = previews.iter().map(|preview| preview.size).sum();
        for preview in previews {
            if remaining <= state.limit_bytes {
                break;
            }
            if Some(preview.path.as_path()) == keep {
                continue;
            }
            match std::fs::remove_file(&preview.path) {
                Ok(()) => remaining -= preview.size,
                Err(error) => {
                    tracing::warn!(%error, path = %preview.path.display(), "cannot remove a preview");
                }
            }
        }
        tracing::info!(before = used, after = remaining, "preview cache trimmed");
        state.used_bytes = Some(remaining);
    }

    /// Aperçus présents dans le cache (dossiers `000`, `001`...).
    fn list(&self) -> Vec<CachedPreview> {
        let Ok(folders) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        folders
            .filter_map(Result::ok)
            .filter_map(|folder| std::fs::read_dir(folder.path()).ok())
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some(PREVIEW_EXTENSION)
            })
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then(|| CachedPreview {
                    path: entry.path(),
                    size: metadata.len(),
                    last_used: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                })
            })
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Un verrou « empoisonné » garde des chiffres cohérents : au pire, la
        // place occupée sera remesurée.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Note l'aperçu comme « regardé à l'instant », dans sa date de
/// modification : c'est elle qui ordonne les effacements. Un échec est sans
/// gravité (l'aperçu risque seulement d'être effacé plus tôt).
fn mark_used(path: &Path) {
    let touched = File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(error) = touched {
        tracing::debug!(%error, path = %path.display(), "cannot mark a preview as used");
    }
}
