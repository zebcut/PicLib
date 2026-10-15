//! Mise à la corbeille de médias : les fichiers vont dans la
//! corbeille du système, un par un. Chacun est noté « en attente » au
//! journal avant d'être déplacé, puis retiré du catalogue dans la même
//! transaction que son résultat au journal : le catalogue
//! suit le disque fichier par fichier.
//!
//! Si l'opération s'arrête en cours de route (annulation, erreur), seuls les
//! médias réellement mis à la corbeille ont été retirés. Après un arrêt
//! brutal entre le déplacement et le retrait, la ligne restée en attente
//! permet de terminer à l'ouverture suivante (`interrupted`). Un fichier
//! introuvable ou refusé par le système reste au catalogue.
//!
//! Les fichiers annexes (`.xmp`, `.dop`) suivent leur photo dans la
//! corbeille, chacun noté au journal.

mod folder;

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use crate::catalog::repositories::file_operations::{
    self, ItemId, ItemResult, OperationKind, OperationStatus,
};
use crate::catalog::repositories::removal;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaId, Timestamp};
use crate::files::{Trash, TrashOutcome, trash_file};
use crate::jobs::CancellationToken;

pub use folder::{FolderTrashOutcome, trash_folder};

/// Média à placer dans la corbeille.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    /// Identifiant du média.
    pub media_id: MediaId,
    /// Chemin complet du fichier.
    pub path: PathBuf,
    /// Fichiers annexes, mis à la corbeille après lui.
    pub sidecars: Vec<PathBuf>,
}

impl From<crate::media_files::MediaFiles> for TrashItem {
    fn from(files: crate::media_files::MediaFiles) -> Self {
        Self {
            media_id: files.media_id,
            path: files.path,
            sidecars: files.sidecars,
        }
    }
}

/// Bilan d'une mise à la corbeille.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrashReport {
    /// Fichiers placés dans la corbeille (et médias retirés du catalogue).
    pub trashed: u64,
    /// Fichiers introuvables, laissés au catalogue.
    pub missing: u64,
    /// Fichiers refusés par le système, laissés à leur place et au catalogue.
    pub failed: u64,
}

/// Erreur qui interrompt une mise à la corbeille.
#[derive(Debug, thiserror::Error)]
pub enum TrashMediaError {
    /// Arrêtée à la demande de l'utilisateur.
    #[error("cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Place les fichiers de `items` dans la corbeille, un par un.
///
/// `on_progress` reçoit le nombre de fichiers traités ; `on_removed` les
/// médias retirés du catalogue, dès leur retrait (pour effacer leurs
/// vignettes).
///
/// # Errors
///
/// Voir [`TrashMediaError`] ; les fichiers déjà traités le restent.
pub fn trash_media(
    catalog: &Mutex<Catalog>,
    trash: &dyn Trash,
    items: &[TrashItem],
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64),
    mut on_removed: impl FnMut(&[MediaId]),
) -> Result<TrashReport, TrashMediaError> {
    let operation = {
        let catalog = lock(catalog);
        file_operations::start(
            catalog.connection(),
            OperationKind::Trash,
            None,
            Timestamp::now(),
        )?
    };
    let mut report = TrashReport::default();
    let mut done = 0;
    let result = (|| {
        for item in items {
            if cancellation.is_cancelled() {
                return Err(TrashMediaError::Cancelled);
            }
            // L'intention d'abord : après un arrêt brutal, elle dit quel
            // fichier était en train de partir.
            let pending = {
                let catalog = lock(catalog);
                file_operations::record_pending(catalog.connection(), operation, &item.path, None)?
            };
            // Le fichier est déplacé sans tenir le verrou du catalogue.
            let outcome = trash_file(trash, &item.path);
            if record_outcome(catalog, pending, item, &outcome, &mut report)? {
                on_removed(&[item.media_id]);
                trash_sidecars(catalog, trash, operation, &item.sidecars)?;
            }
            done += 1;
            on_progress(done);
        }
        Ok(())
    })();

    let status = match &result {
        Ok(()) => OperationStatus::Completed,
        Err(TrashMediaError::Cancelled) => OperationStatus::Cancelled,
        Err(TrashMediaError::Catalog(_)) => OperationStatus::Failed,
    };
    let catalog_guard = lock(catalog);
    if let Err(error) = file_operations::finish(
        catalog_guard.connection(),
        operation,
        status,
        Timestamp::now(),
    ) {
        tracing::error!(%error, "cannot close the file operation in the journal");
    }
    result.map(|()| report)
}

/// Place les fichiers annexes d'une photo partie dans la corbeille, chacun
/// noté au journal ; un échec est noté sans arrêter l'opération.
fn trash_sidecars(
    catalog: &Mutex<Catalog>,
    trash: &dyn Trash,
    operation: file_operations::OperationId,
    sidecars: &[PathBuf],
) -> Result<(), CatalogError> {
    for sidecar in sidecars {
        let pending =
            file_operations::record_pending(lock(catalog).connection(), operation, sidecar, None)?;
        let (result, destination, detail) = match trash_file(trash, sidecar) {
            TrashOutcome::Trashed(location) => (ItemResult::Done, location, None),
            TrashOutcome::Missing => (ItemResult::Missing, None, None),
            TrashOutcome::Failed(reason) => (ItemResult::Failed, None, Some(reason)),
        };
        file_operations::complete(
            lock(catalog).connection(),
            pending,
            destination.as_deref(),
            result,
            detail.as_deref(),
        )?;
    }
    Ok(())
}

/// Note au journal le résultat d'un fichier et, s'il est dans la
/// corbeille, le retire du catalogue, dans une seule transaction ; renvoie
/// vrai s'il a été retiré.
fn record_outcome(
    catalog: &Mutex<Catalog>,
    pending: ItemId,
    item: &TrashItem,
    outcome: &TrashOutcome,
    report: &mut TrashReport,
) -> Result<bool, CatalogError> {
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let (result, destination, detail) = match outcome {
        TrashOutcome::Trashed(location) => (ItemResult::Done, location.as_deref(), None),
        TrashOutcome::Missing => (ItemResult::Missing, None, None),
        TrashOutcome::Failed(reason) => (ItemResult::Failed, None, Some(reason.as_str())),
    };
    file_operations::complete(&transaction, pending, destination, result, detail)?;
    let trashed = matches!(outcome, TrashOutcome::Trashed(_));
    if trashed {
        removal::delete(&transaction, &[item.media_id])?;
    }
    transaction.commit()?;
    match outcome {
        TrashOutcome::Trashed(_) => report.trashed += 1,
        TrashOutcome::Missing => report.missing += 1,
        TrashOutcome::Failed(_) => report.failed += 1,
    }
    Ok(trashed)
}

fn lock(catalog: &Mutex<Catalog>) -> std::sync::MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
