//! Fabrication des vignettes manquantes, en parallèle.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::cache::{CacheError, ThumbnailCache};
use crate::catalog::repositories::thumbnails::{self, PendingThumbnail, ReadyThumbnail};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::Timestamp;
use crate::jobs::CancellationToken;
use crate::platform::{ThumbnailError, ThumbnailInfo};

/// Nombre de vignettes fabriquées entre deux écritures au catalogue.
const BATCH_SIZE: usize = 64;

/// A batch in which no thumbnail could be written means the cache itself is
/// unwritable (disk full, disk unplugged): the pass stops. Below this size
/// it proves nothing (the last few stubborn files of a pass).
const MIN_BATCH_TO_STOP: usize = 8;

/// Bilan d'une fabrication de vignettes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenerationReport {
    /// Vignettes fabriquées.
    pub generated: u64,
    /// Photos ou vidéos que le système ne sait pas décoder, even on a
    /// second try (recorded; Library ▸ "Retry Failed Thumbnails" tries them
    /// again).
    pub failed: u64,
    /// Photos introuvables (disque débranché...) : retentées plus tard.
    pub unavailable: u64,
    /// Thumbnails that could not be written even on a second try: left to
    /// do, retried at the next pass.
    pub not_written: u64,
}

/// Erreur qui interrompt la fabrication.
#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    /// L'utilisateur a annulé ; les vignettes déjà faites sont gardées.
    #[error("thumbnail generation cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Le cache ne peut pas être écrit (disque plein...), or this system
    /// has no decoder: inutile de continuer.
    #[error(transparent)]
    Cache(CacheError),
}

/// Issue de la fabrication d'une vignette.
enum Outcome {
    Ready(ThumbnailInfo),
    /// The system cannot decode the file.
    Failed,
    /// The file is not there (disk unplugged...).
    Unavailable,
    /// The thumbnail could not be written.
    NotWritten(CacheError),
}

impl Outcome {
    /// A failure that may be passing (a sync client holding the file,
    /// memory pressure while large files are decoded in parallel).
    fn deserves_retry(&self) -> bool {
        matches!(self, Self::Failed | Self::NotWritten(_))
    }
}

/// Fabrique les vignettes de toutes les photos et vidéos qui n'en ont pas,
/// avec
/// `workers` fils d'exécution en parallèle. `on_progress` reçoit le nombre de
/// photos traitées et le total.
///
/// # Errors
///
/// Voir [`GenerationError`].
pub fn generate_pending(
    catalog: &Mutex<Catalog>,
    cache: &ThumbnailCache,
    workers: usize,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<GenerationReport, GenerationError> {
    let pending = thumbnails::pending(lock(catalog).connection())?;
    let total = pending.len() as u64;
    let mut report = GenerationReport::default();
    let mut processed = 0_u64;
    on_progress(0, total);

    for batch in pending.chunks(BATCH_SIZE) {
        if cancellation.is_cancelled() {
            return Err(GenerationError::Cancelled);
        }
        // Les vignettes sont fabriquées sans bloquer le catalogue.
        let mut outcomes = make_in_parallel(batch, cache, workers, cancellation)?;
        retry_alone(&mut outcomes, cache, cancellation)?;
        stop_if_cache_unwritable(&mut outcomes)?;
        log_failures(&outcomes);
        record(catalog, &outcomes, &mut report)?;
        processed += batch.len() as u64;
        on_progress(processed, total);
    }
    Ok(report)
}

fn make_in_parallel<'p>(
    batch: &'p [PendingThumbnail],
    cache: &ThumbnailCache,
    workers: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<(&'p PendingThumbnail, Outcome)>, GenerationError> {
    let next = AtomicUsize::new(0);
    let worker_results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.clamp(1, batch.len().max(1)))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    while !cancellation.is_cancelled() {
                        let Some(pending) = batch.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            break;
                        };
                        done.push((pending, make_one(cache, pending)));
                    }
                    done
                })
            })
            .collect();
        // Un fil qui panique (décodeur défaillant) perd seulement ses
        // vignettes, qui resteront à faire au prochain passage.
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect::<Vec<_>>()
    });

    let mut outcomes = Vec::with_capacity(batch.len());
    for result in worker_results.into_iter().flatten() {
        let (pending, outcome) = result;
        outcomes.push((pending, outcome.map_err(GenerationError::Cache)?));
    }
    Ok(outcomes)
}

/// Tries the failed thumbnails of a batch once more, one at a time: a
/// failure caused by the parallel work or by a passing lock on the file
/// then goes away.
fn retry_alone(
    outcomes: &mut [(&PendingThumbnail, Outcome)],
    cache: &ThumbnailCache,
    cancellation: &CancellationToken,
) -> Result<(), GenerationError> {
    for (pending, outcome) in outcomes.iter_mut() {
        if !outcome.deserves_retry() {
            continue;
        }
        if cancellation.is_cancelled() {
            return Err(GenerationError::Cancelled);
        }
        *outcome = make_one(cache, pending).map_err(GenerationError::Cache)?;
    }
    Ok(())
}

/// Stops the pass when no thumbnail of a full batch could be written.
fn stop_if_cache_unwritable(
    outcomes: &mut Vec<(&PendingThumbnail, Outcome)>,
) -> Result<(), GenerationError> {
    let all_unwritten = outcomes.len() >= MIN_BATCH_TO_STOP
        && outcomes
            .iter()
            .all(|(_, outcome)| matches!(outcome, Outcome::NotWritten(_)));
    if !all_unwritten {
        return Ok(());
    }
    match outcomes.pop() {
        Some((_, Outcome::NotWritten(error))) => Err(GenerationError::Cache(error)),
        _ => Ok(()),
    }
}

fn log_failures(outcomes: &[(&PendingThumbnail, Outcome)]) {
    for (pending, outcome) in outcomes {
        match outcome {
            Outcome::Failed => {
                tracing::warn!(path = %pending.path.display(), "media cannot be decoded, no thumbnail");
            }
            Outcome::NotWritten(error) => {
                tracing::warn!(
                    path = %pending.path.display(),
                    %error,
                    "thumbnail cannot be written, left for the next pass"
                );
            }
            Outcome::Ready(_) | Outcome::Unavailable => {}
        }
    }
}

/// Makes one thumbnail. Only a missing decoder for this system is an
/// error: every other failure concerns this file alone.
fn make_one(cache: &ThumbnailCache, pending: &PendingThumbnail) -> Result<Outcome, CacheError> {
    if !pending.path.is_file() {
        return Ok(Outcome::Unavailable);
    }
    match cache.create(pending) {
        Ok(info) => Ok(Outcome::Ready(info)),
        Err(CacheError::Thumbnail(ThumbnailError::Undecodable | ThumbnailError::InvalidPath)) => {
            Ok(Outcome::Failed)
        }
        Err(error @ CacheError::Thumbnail(ThumbnailError::Unsupported)) => Err(error),
        Err(error) => Ok(Outcome::NotWritten(error)),
    }
}

/// Enregistre un lot de résultats dans une seule transaction.
fn record(
    catalog: &Mutex<Catalog>,
    outcomes: &[(&PendingThumbnail, Outcome)],
    report: &mut GenerationReport,
) -> Result<(), CatalogError> {
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let now = Timestamp::now();
    for (pending, outcome) in outcomes {
        match outcome {
            Outcome::Ready(info) => {
                let ready = ReadyThumbnail {
                    width: info.width,
                    height: info.height,
                    from_embedded_preview: info.used_embedded_preview,
                };
                thumbnails::record_ready(&transaction, pending, &ready, now)?;
                if let (Some(width), Some(height)) = (info.source_width, info.source_height) {
                    thumbnails::fill_missing_dimensions(
                        &transaction,
                        pending.media_id,
                        width,
                        height,
                    )?;
                }
                report.generated += 1;
            }
            Outcome::Failed => {
                thumbnails::record_failed(&transaction, pending, now)?;
                report.failed += 1;
            }
            Outcome::Unavailable => report.unavailable += 1,
            Outcome::NotWritten(_) => report.not_written += 1,
        }
    }
    transaction.commit()?;
    Ok(())
}

/// Verrouille le catalogue (un verrou « empoisonné » protège quand même une
/// connexion utilisable : chaque écriture est dans une transaction).
fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
