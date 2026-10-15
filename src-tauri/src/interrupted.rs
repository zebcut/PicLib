//! Reprise des opérations interrompues : après un arrêt
//! brutal, le catalogue est remis d'accord avec le disque, d'après les
//! lignes du journal restées « en attente ».
//!
//! - **Copie** : une copie vérifiée puis nommée (son empreinte le prouve)
//!   est inscrite au catalogue ; une copie jamais vérifiée est abandonnée,
//!   et les fichiers provisoires de PicLib laissés dans ses dossiers sont
//!   effacés.
//! - **Corbeille** : un fichier (ou un dossier entier, avec ses médias) qui
//!   n'est plus à sa place est retiré du catalogue ; resté en place, il y
//!   reste.
//! - **Déplacement de dossier** : un dossier arrivé à destination y est
//!   inscrit au catalogue ; resté en place, il y reste.
//! - Un disque débranché laisse ses lignes en attente, pour la fois
//!   suivante.
//! - Les imports et les opérations restés « en cours » sont clos, comme en
//!   échec.
//!
//! Rien n'est jamais effacé sur le disque, hormis les fichiers provisoires
//! créés par PicLib.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::tz::TimeZone;

use crate::catalog::repositories::file_operations::{
    self, InterruptedOperation, ItemResult, OperationKind, PendingItem,
};
use crate::catalog::repositories::{folders, imports, media, removal};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaId, Timestamp};
use crate::files::{file_digest, remove_partial_copies};
use crate::import::{ImportError, RecoveredCopy, register_recovered_copies};

/// Motif noté au journal pour un fichier dont l'opération a été
/// interrompue sans rien faire sur le disque.
const INTERRUPTED: &str = "interrupted";

/// Bilan de la reprise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumeReport {
    /// Opérations reprises.
    pub operations: u64,
    /// Copies retrouvées et inscrites au catalogue (fichiers annexes
    /// compris).
    pub copies_registered: u64,
    /// Copies jamais terminées, abandonnées.
    pub copies_abandoned: u64,
    /// Médias retirés du catalogue : leur fichier était parti dans la
    /// corbeille.
    pub removed_from_catalog: u64,
    /// Fichiers restés en place, gardés au catalogue.
    pub kept_in_catalog: u64,
    /// Dossiers arrivés à destination, inscrits au catalogue.
    pub folders_moved: u64,
    /// Photos arrivées à destination (déplacées ou renommées), inscrites au
    /// catalogue.
    pub files_moved: u64,
    /// Lignes laissées en attente : disque débranché ou fichier illisible.
    pub left_pending: u64,
    /// Fichiers provisoires de PicLib effacés.
    pub partial_files_removed: u64,
    /// Imports restés « en cours », clos.
    pub imports_closed: u64,
}

impl ResumeReport {
    /// Vrai s'il n'y avait rien à reprendre.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Vrai si le catalogue a reçu ou perdu des médias.
    #[must_use]
    pub fn changed_media(&self) -> bool {
        self.copies_registered > 0 || self.removed_from_catalog > 0
    }
}

/// Erreur qui interrompt la reprise ; elle recommencera à l'ouverture
/// suivante.
#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Les copies retrouvées n'ont pas pu être inscrites.
    #[error(transparent)]
    Import(#[from] ImportError),
}

/// Reprend les opérations interrompues. Les opérations et les imports
/// commencés après l'appel ne sont pas touchés : l'application peut
/// continuer pendant la reprise.
///
/// # Errors
///
/// Voir [`ResumeError`] ; ce qui a été repris le reste.
pub fn resume_interrupted(
    catalog: &Mutex<Catalog>,
    time_zone: &TimeZone,
) -> Result<ResumeReport, ResumeError> {
    let (operations, running_imports) = {
        let catalog = lock(catalog);
        (
            file_operations::interrupted(catalog.connection())?,
            imports::running(catalog.connection())?,
        )
    };
    let mut report = ResumeReport::default();
    for operation in operations {
        report.operations += 1;
        let pending = file_operations::pending_items(lock(catalog).connection(), operation.id)?;
        match operation.kind {
            OperationKind::Trash => {
                for item in &pending {
                    resume_trash(catalog, item, &mut report)?;
                }
            }
            OperationKind::Move => {
                for item in &pending {
                    resume_move(catalog, item, &mut report)?;
                }
            }
            OperationKind::Copy => {
                resume_copy(catalog, operation, &pending, time_zone, &mut report)?;
            }
            OperationKind::Backup => resume_backup(catalog, operation, &pending, &mut report)?,
        }
        file_operations::close_interrupted(
            lock(catalog).connection(),
            operation.id,
            Timestamp::now(),
        )?;
    }
    for import in running_imports {
        imports::close_interrupted(lock(catalog).connection(), import, Timestamp::now())?;
        report.imports_closed += 1;
    }
    if !report.is_empty() {
        tracing::info!(?report, "interrupted operations resumed");
    }
    Ok(report)
}

/// Fichier qui partait dans la corbeille : parti, il quitte le catalogue ;
/// resté en place, il y reste.
fn resume_trash(
    catalog: &Mutex<Catalog>,
    item: &PendingItem,
    report: &mut ResumeReport,
) -> Result<(), CatalogError> {
    let Some(folder) = item.source.parent().filter(|folder| folder.is_dir()) else {
        // Disque débranché : on ne peut pas savoir, on attend.
        report.left_pending += 1;
        return Ok(());
    };
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    if item.source.exists() {
        file_operations::complete(
            &transaction,
            item.id,
            None,
            ItemResult::Failed,
            Some(INTERRUPTED),
        )?;
        report.kept_in_catalog += 1;
    } else {
        // Un dossier entier(panneau Dossiers), ou un fichier.
        let gone: Vec<MediaId> = if let Some(trashed) =
            folders::find_by_path(&transaction, &item.source)?
        {
            folders::media_in_branch(&transaction, trashed.id)?
                .into_iter()
                .map(|media| media.id)
                .collect()
        } else {
            let name = item.source.file_name().and_then(|name| name.to_str());
            match (folders::find_by_path(&transaction, folder)?, name) {
                (Some(folder), Some(name)) => media::find_in_folder(&transaction, folder.id, name)?
                    .map(|found| found.id)
                    .into_iter()
                    .collect(),
                _ => Vec::new(),
            }
        };
        removal::delete(&transaction, &gone)?;
        report.removed_from_catalog += gone.len() as u64;
        file_operations::complete(&transaction, item.id, None, ItemResult::Done, None)?;
    }
    transaction.commit()?;
    Ok(())
}

/// Dossier qui se déplaçait : arrivé à destination, le catalogue le suit ;
/// resté en place, rien ne change.
fn resume_move(
    catalog: &Mutex<Catalog>,
    item: &PendingItem,
    report: &mut ResumeReport,
) -> Result<(), CatalogError> {
    let Some(destination) = &item.destination else {
        return abandon(catalog, item, INTERRUPTED);
    };
    let is_online = |path: &Path| path.parent().is_some_and(Path::is_dir);
    if !is_online(&item.source) || !is_online(destination) {
        // Disque débranché : on ne peut pas savoir, on attend.
        report.left_pending += 1;
        return Ok(());
    }
    if !item.source.exists() && destination.is_file() {
        return resume_file_move(catalog, item, destination, report);
    }
    if item.source.exists() || !destination.is_dir() {
        report.kept_in_catalog += 1;
        return abandon(catalog, item, INTERRUPTED);
    }
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    if let (Some(moved), Some(parent)) = (
        folders::find_by_path(&transaction, &item.source)?,
        destination.parent(),
    ) {
        let parent = folders::ensure_path(&transaction, parent)?;
        folders::relocate(&transaction, moved.id, destination, Some(parent))?;
        folders::remove_without_media(&transaction)?;
        report.folders_moved += 1;
    }
    file_operations::complete(&transaction, item.id, None, ItemResult::Done, None)?;
    transaction.commit()?;
    Ok(())
}

/// Photo (ou fichier annexe) qui se déplaçait ou se renommait : arrivée à destination, le catalogue la suit.
fn resume_file_move(
    catalog: &Mutex<Catalog>,
    item: &PendingItem,
    destination: &Path,
    report: &mut ResumeReport,
) -> Result<(), CatalogError> {
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let name = item.source.file_name().and_then(|name| name.to_str());
    let parent = item.source.parent();
    let found = match (parent, name) {
        (Some(parent), Some(name)) => match folders::find_by_path(&transaction, parent)? {
            Some(folder) => media::find_in_folder(&transaction, folder.id, name)?,
            None => None,
        },
        _ => None,
    };
    if let (Some(found), Some(new_folder), Some(new_name)) = (
        found,
        destination.parent(),
        destination.file_name().and_then(|name| name.to_str()),
    ) {
        let folder = folders::ensure_path(&transaction, new_folder)?;
        media::set_place(&transaction, found.id, folder, new_name)?;
        folders::remove_without_media(&transaction)?;
        report.files_moved += 1;
    }
    file_operations::complete(&transaction, item.id, None, ItemResult::Done, None)?;
    transaction.commit()?;
    Ok(())
}

/// Copies d'un import interrompu : celles qui ont été vérifiées et nommées
/// sont inscrites ; les autres sont abandonnées ; les fichiers provisoires
/// sont effacés.
fn resume_copy(
    catalog: &Mutex<Catalog>,
    operation: InterruptedOperation,
    pending: &[PendingItem],
    time_zone: &TimeZone,
    report: &mut ResumeReport,
) -> Result<(), ResumeError> {
    let mut recovered = Vec::new();
    for item in pending {
        let (Some(content_hash), Some(destination)) = (&item.content_hash, &item.destination)
        else {
            // Jamais vérifiée : rien n'a été nommé.
            abandon(catalog, item, INTERRUPTED)?;
            report.copies_abandoned += 1;
            continue;
        };
        if !destination.parent().is_some_and(Path::is_dir) {
            report.left_pending += 1;
            continue;
        }
        if !destination.is_file() {
            // Interrompue avant d'être nommée.
            abandon(catalog, item, INTERRUPTED)?;
            report.copies_abandoned += 1;
            continue;
        }
        match file_digest(destination) {
            Ok((_, digest)) if digest.to_hex() == *content_hash => recovered.push(RecoveredCopy {
                item: item.id,
                path: destination.clone(),
                content_hash: content_hash.clone(),
            }),
            Ok(_) => {
                // Un autre fichier a pris ce nom : ce n'est pas la copie.
                abandon(catalog, item, "another file has this name")?;
                report.copies_abandoned += 1;
            }
            Err(error) => {
                tracing::warn!(path = %destination.display(), %error, "copy left pending");
                report.left_pending += 1;
            }
        }
    }

    if !recovered.is_empty() {
        let destination_root = match operation.import_id {
            Some(import) => imports::find(lock(catalog).connection(), import)?
                .and_then(|import| import.destination_path)
                .map(|root| (import, root)),
            None => None,
        };
        match destination_root {
            Some((import, root)) => {
                register_recovered_copies(catalog, import, &root, &recovered, time_zone)?;
                report.copies_registered += recovered.len() as u64;
            }
            None => report.left_pending += recovered.len() as u64,
        }
    }

    let folders = file_operations::destination_folders(lock(catalog).connection(), operation.id)?;
    for folder in folders.iter().filter(|folder| folder.is_dir()) {
        match remove_partial_copies(folder) {
            Ok(removed) => report.partial_files_removed += removed as u64,
            Err(error) => {
                tracing::warn!(folder = %folder.display(), %error, "cannot remove partial copies");
            }
        }
    }
    Ok(())
}

/// Copies de sauvegarde d'un import interrompu : celles qui
/// ont été vérifiées et nommées sont faites (elles n'entrent pas au
/// catalogue) ; les autres sont abandonnées ; les fichiers provisoires sont
/// effacés.
fn resume_backup(
    catalog: &Mutex<Catalog>,
    operation: InterruptedOperation,
    pending: &[PendingItem],
    report: &mut ResumeReport,
) -> Result<(), ResumeError> {
    for item in pending {
        let (Some(content_hash), Some(destination)) = (&item.content_hash, &item.destination)
        else {
            abandon(catalog, item, INTERRUPTED)?;
            report.copies_abandoned += 1;
            continue;
        };
        if !destination.parent().is_some_and(Path::is_dir) {
            report.left_pending += 1;
            continue;
        }
        match file_digest(destination) {
            Ok((_, digest)) if digest.to_hex() == *content_hash => {
                file_operations::complete(
                    lock(catalog).connection(),
                    item.id,
                    Some(destination),
                    ItemResult::Done,
                    None,
                )?;
            }
            Ok(_) => {
                abandon(catalog, item, "another file has this name")?;
                report.copies_abandoned += 1;
            }
            Err(error) if destination.is_file() => {
                tracing::warn!(path = %destination.display(), %error, "backup left pending");
                report.left_pending += 1;
            }
            Err(_) => {
                // Interrompue avant d'être nommée.
                abandon(catalog, item, INTERRUPTED)?;
                report.copies_abandoned += 1;
            }
        }
    }
    let folders = file_operations::destination_folders(lock(catalog).connection(), operation.id)?;
    for folder in folders.iter().filter(|folder| folder.is_dir()) {
        match remove_partial_copies(folder) {
            Ok(removed) => report.partial_files_removed += removed as u64,
            Err(error) => {
                tracing::warn!(folder = %folder.display(), %error, "cannot remove partial copies");
            }
        }
    }
    Ok(())
}

/// Clôt une copie qui n'a rien laissé à inscrire.
fn abandon(catalog: &Mutex<Catalog>, item: &PendingItem, detail: &str) -> Result<(), CatalogError> {
    file_operations::complete(
        lock(catalog).connection(),
        item.id,
        None,
        ItemResult::Failed,
        Some(detail),
    )
}

/// Verrouille le catalogue, même « empoisonné » : chaque écriture est dans
/// une transaction.
fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
