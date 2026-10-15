//! Fonds de carte de la vue Carte : les images
//! (« tuiles ») d'OpenStreetMap, téléchargées par le cœur et gardées dans un
//! cache sur disque. L'interface ne contacte jamais Internet elle-même : elle
//! demande `piclib://…/tile/<z>/<x>/<y>` (voir `protocol`).
//!
//! Règles d'OpenStreetMap respectées : application identifiée (en-tête
//! `User-Agent`), deux téléchargements au plus à la fois, tuiles gardées en
//! cache (une zone déjà vue s'affiche encore hors connexion), mention
//! « © OpenStreetMap » sur la carte. Aucune donnée du catalogue n'est
//! envoyée : seules les coordonnées des tuiles le sont.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

/// Zoom maximal des tuiles d'OpenStreetMap.
pub const MAX_ZOOM: u8 = 19;

/// Place maximale du cache des tuiles : 200 Mo, des milliers de tuiles.
pub const CACHE_LIMIT_BYTES: u64 = 200 * 1024 * 1024;

/// Au-delà, une tuile en cache est relue sur Internet (la carte change peu).
const REFRESH_AFTER: Duration = Duration::from_hours(30 * 24);

/// Taille maximale d'une tuile téléchargée.
const MAX_TILE_BYTES: u64 = 1024 * 1024;

/// Téléchargements en même temps, au plus (règle d'OpenStreetMap).
const MAX_PARALLEL_DOWNLOADS: usize = 2;

/// Adresse des tuiles d'OpenStreetMap.
const TILE_URL: &str = "https://tile.openstreetmap.org";

/// Début de tout fichier PNG.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Numéro des fichiers provisoires des tuiles.
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Coordonnées d'une tuile : zoom, colonne et rangée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileCoord {
    /// Zoom (0 : le monde entier en une tuile).
    pub z: u8,
    /// Colonne.
    pub x: u32,
    /// Rangée.
    pub y: u32,
}

impl TileCoord {
    /// Tuile, si ses coordonnées existent à ce zoom.
    #[must_use]
    pub fn new(z: u8, x: u32, y: u32) -> Option<Self> {
        let size = 1_u64 << z.min(MAX_ZOOM);
        (z <= MAX_ZOOM && u64::from(x) < size && u64::from(y) < size).then_some(Self { z, x, y })
    }

    fn cache_path(self, root: &Path) -> PathBuf {
        root.join(self.z.to_string())
            .join(self.x.to_string())
            .join(format!("{}.png", self.y))
    }
}

/// Échec d'une tuile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TileError {
    /// Pas de connexion, ou le serveur a refusé, et la tuile n'est pas en
    /// cache.
    #[error("map tile unavailable: {0}")]
    Unavailable(String),
}

/// D'où viennent les tuiles (Internet, ou un faux serveur dans les tests).
pub trait TileSource: Send + Sync {
    /// Télécharge une tuile (image PNG).
    ///
    /// # Errors
    ///
    /// Voir [`TileError`].
    fn fetch(&self, tile: TileCoord) -> Result<Vec<u8>, TileError>;
}

/// Tuiles d'OpenStreetMap, téléchargées deux à la fois au plus.
pub struct OsmTiles {
    agent: ureq::Agent,
    /// Téléchargements en cours.
    running: Mutex<usize>,
    freed: Condvar,
}

impl OsmTiles {
    /// Source identifiée par `user_agent` (nom et version de PicLib).
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .timeout_global(Some(Duration::from_secs(15)))
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::NativeTls)
                    .build(),
            )
            .build();
        Self {
            agent: config.new_agent(),
            running: Mutex::new(0),
            freed: Condvar::new(),
        }
    }
}

impl std::fmt::Debug for OsmTiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OsmTiles")
    }
}

impl TileSource for OsmTiles {
    fn fetch(&self, tile: TileCoord) -> Result<Vec<u8>, TileError> {
        {
            let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
            while *running >= MAX_PARALLEL_DOWNLOADS {
                running = self
                    .freed
                    .wait(running)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            *running += 1;
        }
        let url = format!("{TILE_URL}/{}/{}/{}.png", tile.z, tile.x, tile.y);
        let result = self
            .agent
            .get(&url)
            .call()
            .and_then(|mut response| {
                response
                    .body_mut()
                    .with_config()
                    .limit(MAX_TILE_BYTES)
                    .read_to_vec()
            })
            .map_err(|error| TileError::Unavailable(error.to_string()));
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
        self.freed.notify_one();
        result
    }
}

/// Cache des tuiles sur disque, limité en taille : au-delà, les tuiles les
/// moins récemment téléchargées sont effacées (ce ne sont que des images
/// d'OpenStreetMap, jamais des fichiers de l'utilisateur).
pub struct MapTileCache {
    root: PathBuf,
    limit_bytes: u64,
    source: Box<dyn TileSource>,
    /// Place occupée, mesurée au premier besoin puis tenue à jour.
    used_bytes: Mutex<Option<u64>>,
}

impl std::fmt::Debug for MapTileCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MapTileCache")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl MapTileCache {
    /// Cache rangé dans `root`, limité à `limit_bytes`, rempli depuis
    /// `source`.
    #[must_use]
    pub fn new(root: PathBuf, limit_bytes: u64, source: Box<dyn TileSource>) -> Self {
        Self {
            root,
            limit_bytes,
            source,
            used_bytes: Mutex::new(None),
        }
    }

    /// Image d'une tuile : celle du cache si elle est récente, sinon
    /// téléchargée (et gardée) ; hors connexion, celle du cache même
    /// ancienne.
    ///
    /// # Errors
    ///
    /// [`TileError::Unavailable`] si elle n'est ni téléchargeable ni en
    /// cache.
    pub fn get(&self, tile: TileCoord) -> Result<Vec<u8>, TileError> {
        let path = tile.cache_path(&self.root);
        let cached = std::fs::read(&path).ok();
        let is_fresh = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < REFRESH_AFTER);
        if let Some(bytes) = &cached
            && is_fresh
        {
            return Ok(bytes.clone());
        }
        match self.source.fetch(tile) {
            // Une page reçue à la place d'une image (réseau Wi-Fi qui
            // demande une connexion) n'est ni gardée ni affichée.
            Ok(bytes) if !bytes.starts_with(PNG_SIGNATURE) => cached.ok_or_else(|| {
                TileError::Unavailable("the server did not send a PNG image".to_owned())
            }),
            Ok(bytes) => {
                self.store(&path, &bytes);
                Ok(bytes)
            }
            Err(error) => cached.ok_or(error),
        }
    }

    /// Range une tuile téléchargée : écrite à côté puis mise en place, le
    /// cache ne garde jamais d'image à moitié écrite.
    fn store(&self, path: &Path, bytes: &[u8]) {
        let Some(folder) = path.parent() else {
            return;
        };
        // Un nom par téléchargement : deux téléchargements de la même tuile
        // n'écrivent jamais dans le même fichier provisoire.
        let number = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_extension(format!("png.{}-{number}.tmp", std::process::id()));
        let written = std::fs::create_dir_all(folder)
            .and_then(|()| std::fs::write(&temporary, bytes))
            .and_then(|()| std::fs::rename(&temporary, path));
        if let Err(error) = written {
            tracing::warn!(%error, path = %path.display(), "cannot cache a map tile");
            let _ = std::fs::remove_file(&temporary);
            return;
        }
        let mut used = self.lock_used();
        let total = used.get_or_insert_with(|| folder_size(&self.root));
        *total += bytes.len() as u64;
        if *total > self.limit_bytes {
            *total = trim(&self.root, self.limit_bytes * 9 / 10);
        }
    }

    fn lock_used(&self) -> MutexGuard<'_, Option<u64>> {
        self.used_bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Tuiles du cache, avec leur taille et leur date.
fn cached_tiles(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "png")
        })
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some((
                entry.path().to_path_buf(),
                metadata.len(),
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ))
        })
        .collect()
}

fn folder_size(root: &Path) -> u64 {
    cached_tiles(root).iter().map(|(_, size, _)| size).sum()
}

/// Efface les tuiles les plus anciennes jusqu'à `target` octets ; renvoie
/// la place encore occupée.
fn trim(root: &Path, target: u64) -> u64 {
    let mut tiles = cached_tiles(root);
    tiles.sort_by_key(|(_, _, modified)| *modified);
    let mut total: u64 = tiles.iter().map(|(_, size, _)| size).sum();
    for (path, size, _) in tiles {
        if total <= target {
            break;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => total = total.saturating_sub(size),
            Err(error) => tracing::warn!(%error, path = %path.display(), "cannot trim a map tile"),
        }
    }
    total
}
