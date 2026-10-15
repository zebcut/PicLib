//! Mise à la corbeille d'un dossier entier depuis le panneau Dossiers, seulement s'il ne contient que des fichiers connus du
//! catalogue. Le dossier est noté « en attente » au journal, placé dans la
//! corbeille, puis ses médias quittent le catalogue dans la même
//! transaction que son résultat au journal.

use std::sync::Mutex;

use super::lock;
use crate::catalog::repositories::file_operations::{
    self, ItemResult, OperationKind, OperationStatus,
};
use crate::catalog::repositories::{folders, removal};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{FolderId, MediaId, Timestamp};
use crate::files::{Trash, TrashOutcome, folder_inventory, trash_file};
use crate::folders::{TrashCheck, folder_contents, trash_check};

/// Résultat de la mise à la corbeille d'un dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderTrashOutcome {
    /// Dans la corbeille ; ses médias ont quitté le catalogue.
    Trashed {
        /// Médias retirés (pour effacer leurs vignettes).
        removed: Vec<MediaId>,
    },
    /// Refusé avant de rien faire : voir la raison.
    Refused(TrashCheck),
    /// Le dossier n'est plus au catalogue.
    Unknown,
    /// Refusé par le système : le dossier est resté à sa place.
    Failed(String),
}

/// Place le dossier `id` dans la corbeille, après avoir vérifié sur le
/// disque qu'il ne contient que des fichiers connus du catalogue.
///
/// # Errors
///
/// Échoue si le catalogue est illisible ou ne peut pas être écrit ; si le
/// dossier était déjà parti, la reprise au démarrage termine.
pub fn trash_folder(
    catalog: &Mutex<Catalog>,
    trash: &dyn Trash,
    id: FolderId,
) -> Result<FolderTrashOutcome, CatalogError> {
    let Some(contents) = folder_contents(lock(catalog).connection(), id)? else {
        return Ok(FolderTrashOutcome::Unknown);
    };
    // Le disque est lu sans tenir le verrou du catalogue.
    let path = contents.folder.path.clone();
    let inventory = path.is_dir().then(|| folder_inventory(&path));
    let check = trash_check(&contents, inventory);
    if check != TrashCheck::Possible {
        return Ok(FolderTrashOutcome::Refused(check));
    }

    let (operation, item) = {
        let catalog = lock(catalog);
        let connection = catalog.connection();
        let operation =
            file_operations::start(connection, OperationKind::Trash, None, Timestamp::now())?;
        let item = file_operations::record_pending(connection, operation, &path, None)?;
        (operation, item)
    };
    let outcome = trash_file(trash, &path);

    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let (result, destination, detail) = match &outcome {
        TrashOutcome::Trashed(location) => (ItemResult::Done, location.as_deref(), None),
        TrashOutcome::Missing => (ItemResult::Missing, None, None),
        TrashOutcome::Failed(reason) => (ItemResult::Failed, None, Some(reason.as_str())),
    };
    file_operations::complete(&transaction, item, destination, result, detail)?;
    let removed = if matches!(outcome, TrashOutcome::Trashed(_)) {
        // Relus maintenant : un média ajouté entre-temps est parti avec le
        // dossier.
        let removed: Vec<MediaId> = folders::media_in_branch(&transaction, id)?
            .into_iter()
            .map(|media| media.id)
            .collect();
        removal::delete(&transaction, &removed)?;
        removed
    } else {
        Vec::new()
    };
    file_operations::finish(
        &transaction,
        operation,
        OperationStatus::Completed,
        Timestamp::now(),
    )?;
    transaction.commit()?;
    Ok(match outcome {
        TrashOutcome::Trashed(_) => FolderTrashOutcome::Trashed { removed },
        TrashOutcome::Missing => FolderTrashOutcome::Refused(TrashCheck::Offline),
        TrashOutcome::Failed(reason) => FolderTrashOutcome::Failed(reason),
    })
}
