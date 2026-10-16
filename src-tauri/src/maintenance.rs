//! Catalog cleanup and optimization (File ▸ "Clean Up and Optimize
//! Catalog…").
//!
//! [`check`] changes nothing: it checks the integrity of the database and
//! counts what a cleanup would remove. [`clean`] starts with a verified
//! backup, then removes the thumbnails and previews of media that are no
//! longer in the catalog (copies made by PicLib, never the user's files),
//! the stacks left with fewer than two photos, and rebuilds the database
//! (`VACUUM`) and its statistics (`ANALYZE`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use crate::catalog::repositories::stacks;
use crate::catalog::{BackupError, Catalog, CatalogError};
use crate::domain::MediaId;
use crate::jobs::CancellationToken;
use crate::thumbnails::{PreviewCache, ThumbnailCache, cached_images};

/// At most this many integrity problems are reported.
const MAX_REPORTED_PROBLEMS: u32 = 10;

/// Caches of the workspace.
#[derive(Debug, Clone, Copy)]
pub struct Caches<'a> {
    /// Folder of the thumbnails.
    pub thumbnails: &'a ThumbnailCache,
    /// Folder of the previews.
    pub previews: &'a PreviewCache,
}

/// What a check found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Integrity problems reported by SQLite (at most ten); empty when the
    /// database is sound.
    pub problems: Vec<String>,
    /// Thumbnails of media no longer in the catalog.
    pub orphan_thumbnails: u64,
    /// Previews of media no longer in the catalog.
    pub orphan_previews: u64,
    /// Size of those thumbnails and previews, in bytes.
    pub orphan_bytes: u64,
    /// Stacks left with fewer than two photos.
    pub lone_stacks: u64,
    /// Size of the database, in bytes.
    pub catalog_bytes: u64,
    /// Unused space in the database, which `VACUUM` gives back.
    pub reclaimable_bytes: u64,
}

/// What a cleanup did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    /// Backup made before anything was changed.
    pub backup: PathBuf,
    /// Thumbnails and previews removed.
    pub removed_images: u64,
    /// Their size, in bytes.
    pub removed_bytes: u64,
    /// Stacks removed.
    pub removed_stacks: u64,
    /// Size of the database before the cleanup, in bytes.
    pub catalog_bytes_before: u64,
    /// Size of the database after the cleanup, in bytes.
    pub catalog_bytes_after: u64,
}

/// Why a check or a cleanup stopped.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    /// The user cancelled.
    #[error("cancelled")]
    Cancelled,
    /// The database is damaged: nothing is changed, a backup should be
    /// restored instead.
    #[error("the catalog failed its integrity check")]
    Damaged,
    /// The backup could not be made or verified: nothing is changed.
    #[error(transparent)]
    Backup(#[from] BackupError),
    /// The database could not be read or written.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

impl From<rusqlite::Error> for MaintenanceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Catalog(error.into())
    }
}

/// Checks the catalog and counts what a cleanup would remove. Nothing is
/// changed. `on_progress` receives the number of cached images examined
/// and their total.
///
/// # Errors
///
/// [`MaintenanceError::Cancelled`], or the database cannot be read.
pub fn check(
    catalog: &Mutex<Catalog>,
    caches: Caches<'_>,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<CheckReport, MaintenanceError> {
    let (problems, known, lone_stacks, (catalog_bytes, reclaimable_bytes)) = {
        let catalog = lock(catalog);
        let connection = catalog.connection();
        (
            integrity_problems(connection, "integrity_check")?,
            KnownMedia::read(connection)?,
            lone_stack_count(connection)?,
            database_sizes(connection)?,
        )
    };
    if cancellation.is_cancelled() {
        return Err(MaintenanceError::Cancelled);
    }
    let thumbnails = cached_images(caches.thumbnails.root());
    let previews = cached_images(caches.previews.root());
    let total = (thumbnails.len() + previews.len()) as u64;
    let mut report = CheckReport {
        problems,
        lone_stacks,
        catalog_bytes,
        reclaimable_bytes,
        ..CheckReport::default()
    };
    for (done, (image, is_thumbnail)) in thumbnails
        .iter()
        .map(|image| (image, true))
        .chain(previews.iter().map(|image| (image, false)))
        .enumerate()
    {
        if cancellation.is_cancelled() {
            return Err(MaintenanceError::Cancelled);
        }
        if known.is_orphan(image.media_id) {
            if is_thumbnail {
                report.orphan_thumbnails += 1;
            } else {
                report.orphan_previews += 1;
            }
            report.orphan_bytes += image.size;
        }
        on_progress(done as u64 + 1, total);
    }
    Ok(report)
}

/// Cleans up the catalog: verified backup into `backups_dir`, removal of
/// the orphan thumbnails and previews and of the lone stacks, then
/// `VACUUM` and `ANALYZE`. `on_progress` receives the number of cached
/// images examined and their total. Cancelling is possible until the
/// database is rebuilt.
///
/// # Errors
///
/// [`MaintenanceError::Damaged`] (nothing is changed),
/// [`MaintenanceError::Backup`] (nothing is changed),
/// [`MaintenanceError::Cancelled`], or the database cannot be written.
pub fn clean(
    catalog: &Mutex<Catalog>,
    caches: Caches<'_>,
    backups_dir: &Path,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<CleanupReport, MaintenanceError> {
    let (backup, known, catalog_bytes_before) = {
        let catalog = lock(catalog);
        let connection = catalog.connection();
        // A quick check is enough here: a damaged database is not touched.
        if !integrity_problems(connection, "quick_check")?.is_empty() {
            return Err(MaintenanceError::Damaged);
        }
        let backup = catalog.back_up_before_cleanup(backups_dir)?;
        tracing::info!(backup = %backup.display(), "catalog backed up before the cleanup");
        (
            backup,
            KnownMedia::read(connection)?,
            database_sizes(connection)?.0,
        )
    };

    let thumbnails = cached_images(caches.thumbnails.root());
    let previews = cached_images(caches.previews.root());
    let total = (thumbnails.len() + previews.len()) as u64;
    let mut removed_images = 0;
    let mut removed_bytes = 0;
    let mut orphan_previews = Vec::new();
    for (done, (image, is_thumbnail)) in thumbnails
        .iter()
        .map(|image| (image, true))
        .chain(previews.iter().map(|image| (image, false)))
        .enumerate()
    {
        if cancellation.is_cancelled() {
            return Err(MaintenanceError::Cancelled);
        }
        if known.is_orphan(image.media_id) {
            if is_thumbnail {
                caches.thumbnails.remove(image.media_id);
            } else {
                orphan_previews.push(image.media_id);
            }
            removed_images += 1;
            removed_bytes += image.size;
        }
        on_progress(done as u64 + 1, total);
    }
    // In one call: the preview cache keeps its used space up to date.
    caches.previews.forget(&orphan_previews);
    if cancellation.is_cancelled() {
        return Err(MaintenanceError::Cancelled);
    }

    let catalog = lock(catalog);
    let connection = catalog.connection();
    let removed_stacks = stacks::remove_lone(connection)? as u64;
    connection.execute_batch("VACUUM; ANALYZE;")?;
    catalog.checkpoint()?;
    let catalog_bytes_after = database_sizes(connection)?.0;
    Ok(CleanupReport {
        backup,
        removed_images,
        removed_bytes,
        removed_stacks,
        catalog_bytes_before,
        catalog_bytes_after,
    })
}

/// Media of the catalog when the check began. A cached image is an orphan
/// when its media is not among them, and is not newer than all of them
/// (media ids are never reused, see `id_watermarks`: a newer id belongs to
/// a photo imported meanwhile, whose thumbnail must stay). Removing the
/// image of a photo whose removal is later undone is harmless: it is made
/// again when needed.
struct KnownMedia {
    ids: HashSet<i64>,
    highest: i64,
}

impl KnownMedia {
    fn read(connection: &Connection) -> Result<Self, MaintenanceError> {
        let ids = connection
            .prepare("SELECT id FROM media")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<HashSet<i64>, _>>()?;
        let highest: i64 = connection.query_row(
            "SELECT max(coalesce((SELECT max(id) FROM media), 0),
                        coalesce((SELECT last_id FROM id_watermarks WHERE name = 'media'), 0))",
            [],
            |row| row.get(0),
        )?;
        Ok(Self { ids, highest })
    }

    fn is_orphan(&self, media_id: MediaId) -> bool {
        let id = media_id.get();
        id <= self.highest && !self.ids.contains(&id)
    }
}

/// Problems reported by `PRAGMA integrity_check` or `quick_check`; empty
/// when the database is sound.
fn integrity_problems(
    connection: &Connection,
    pragma: &str,
) -> Result<Vec<String>, MaintenanceError> {
    let problems = connection
        .prepare(&format!("PRAGMA {pragma}({MAX_REPORTED_PROBLEMS})"))?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if problems == ["ok"] {
        Vec::new()
    } else {
        problems
    })
}

fn lone_stack_count(connection: &Connection) -> Result<u64, MaintenanceError> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM stacks
         WHERE (SELECT count(*) FROM stack_media WHERE stack_media.stack_id = stacks.id) < 2",
        [],
        |row| row.get(0),
    )?;
    Ok(count.unsigned_abs())
}

/// Size of the database and its unused space, in bytes.
fn database_sizes(connection: &Connection) -> Result<(u64, u64), MaintenanceError> {
    let pragma = |name: &str| -> Result<u64, MaintenanceError> {
        let value: i64 = connection.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))?;
        Ok(value.unsigned_abs())
    };
    let page_size = pragma("page_size")?;
    Ok((
        pragma("page_count")? * page_size,
        pragma("freelist_count")? * page_size,
    ))
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
