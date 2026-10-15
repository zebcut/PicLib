//! Déplacer un dossier depuis le panneau Dossiers :
//! dans un autre dossier du catalogue, sur le même disque seulement. Pour le
//! système, ce n'est qu'un renommage, instantané ; le catalogue fait suivre
//! les chemins du dossier et de ses sous-dossiers.
//!
//! Comme toute opération sur les fichiers, le déplacement est
//! noté « en attente » au journal, fait sur le disque, puis inscrit au
//! catalogue dans la même transaction que son résultat. Il s'annule par
//! Cmd+Z : le dossier revient à sa place.

use std::path::{Path, PathBuf};

use crate::catalog::repositories::file_operations::{
    self, ItemResult, OperationKind, OperationStatus,
};
use crate::catalog::repositories::folders;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{Folder, FolderId, Timestamp};
use crate::files::{free_folder_path, move_folder as move_on_disk};
use crate::folders::{FolderContents, folder_contents};
use crate::platform;

mod media;

pub use media::{
    FileGroup, MediaMove, MediaMoveKind, MediaMoveRefusal, MoveOutcome as MediaMoveOutcome,
    MovePlan, execute as execute_media_move, plan_move as plan_media_move,
};
pub(crate) use media::{SourceGroup, free_stem, group_endings, planned_group, source_groups};

/// Raison pour laquelle un dossier ne peut pas être déplacé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveRefusal {
    /// Le dossier est déjà dans la destination.
    AlreadyThere,
    /// La destination est le dossier lui-même ou l'un de ses sous-dossiers.
    IntoItself,
    /// Le dossier est introuvable (disque débranché, dossier déplacé).
    Missing,
    /// La destination est introuvable.
    DestinationMissing,
    /// La destination est sur un autre disque.
    OtherDisk,
    /// Le dossier ou la destination est sur une carte mémoire : rien ne
    /// s'écrit sur une source d'import.
    CameraCard,
}

/// Ce que donnerait le déplacement d'un dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveCheck {
    /// Le dossier, ses sous-dossiers et ses médias.
    pub contents: FolderContents,
    /// Le dossier de destination.
    pub destination: Folder,
    /// Son futur chemin (suffixé `-2`, `-3`… si le nom est pris), ou la
    /// raison du refus.
    pub target: Result<PathBuf, MoveRefusal>,
}

/// Vérifie le déplacement du dossier `id` dans `destination_id` ; `None`
/// si l'un des deux n'est plus au catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn check_move(
    connection: &rusqlite::Connection,
    id: FolderId,
    destination_id: FolderId,
) -> Result<Option<MoveCheck>, CatalogError> {
    let Some(contents) = folder_contents(connection, id)? else {
        return Ok(None);
    };
    let Some(destination) = folders::list(connection)?
        .into_iter()
        .find(|folder| folder.id == destination_id)
    else {
        return Ok(None);
    };
    let target = refusal(&contents.folder, &destination).map_or_else(
        || {
            let name = contents
                .folder
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            Ok(free_folder_path(&destination.path, &name, |candidate| {
                // Base illisible : le nom est tenu pour pris.
                folders::path_in_use(connection, candidate).unwrap_or(true)
            }))
        },
        Err,
    );
    Ok(Some(MoveCheck {
        contents,
        destination,
        target,
    }))
}

fn refusal(folder: &Folder, destination: &Folder) -> Option<MoveRefusal> {
    if folder.parent_id == Some(destination.id) {
        Some(MoveRefusal::AlreadyThere)
    } else if destination.path.starts_with(&folder.path) {
        Some(MoveRefusal::IntoItself)
    } else if !folder.path.is_dir() {
        Some(MoveRefusal::Missing)
    } else if !destination.path.is_dir() {
        Some(MoveRefusal::DestinationMissing)
    } else if platform::is_on_camera_card(&folder.path)
        || platform::is_on_camera_card(&destination.path)
    {
        Some(MoveRefusal::CameraCard)
    } else if !platform::same_volume(&folder.path, &destination.path) {
        Some(MoveRefusal::OtherDisk)
    } else {
        None
    }
}

/// Dossier déplacé : de quoi l'annuler (Cmd+Z) et le rétablir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderMove {
    folder: FolderId,
    from: PathBuf,
    to: PathBuf,
    old_parent: Option<FolderId>,
    new_parent: Option<FolderId>,
    /// Vrai pour un renommage (même parent, autre nom).
    renamed: bool,
    /// Ancien parent et ses parents, tels qu'avant le déplacement : le
    /// catalogue les retire s'ils n'ont plus de média, l'annulation les
    /// remet.
    lineage: Vec<Folder>,
    media_count: usize,
}

impl FolderMove {
    /// Nombre de médias déplacés avec le dossier.
    #[must_use]
    pub fn media_count(&self) -> usize {
        self.media_count
    }

    /// Nouveau chemin du dossier.
    #[must_use]
    pub fn to(&self) -> &Path {
        &self.to
    }

    /// Vrai pour un renommage.
    #[must_use]
    pub fn is_rename(&self) -> bool {
        self.renamed
    }

    /// Remet le dossier à sa place (Cmd+Z).
    ///
    /// # Errors
    ///
    /// Voir [`FolderMoveError`] ; rien n'est alors changé.
    pub fn undo(&self, catalog: &mut Catalog) -> Result<(), FolderMoveError> {
        relocate(catalog, self, &self.to, &self.from, self.old_parent)
    }

    /// Déplace de nouveau le dossier (Cmd+Maj+Z).
    ///
    /// # Errors
    ///
    /// Voir [`FolderMoveError`] ; rien n'est alors changé.
    pub fn redo(&self, catalog: &mut Catalog) -> Result<(), FolderMoveError> {
        relocate(catalog, self, &self.from, &self.to, self.new_parent)
    }
}

/// Échec d'un déplacement, d'une annulation ou d'un rétablissement.
#[derive(Debug, thiserror::Error)]
pub enum FolderMoveError {
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Le système a refusé, ou la place est prise : le dossier n'a pas
    /// bougé.
    #[error("the folder cannot be moved: {0}")]
    Disk(String),
}

/// Résultat d'un déplacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveOutcome {
    /// Déplacé.
    Moved(FolderMove),
    /// Refusé avant de rien faire.
    Refused(MoveRefusal),
    /// Le dossier ou la destination n'est plus au catalogue.
    Unknown,
    /// Le système a refusé : le dossier est resté à sa place.
    Failed(String),
}

/// Déplace le dossier `id` dans `destination_id`, après avoir vérifié que
/// c'est possible.
///
/// # Errors
///
/// Échoue si le catalogue est illisible ou ne peut pas être écrit (le
/// dossier a pu être déplacé : la reprise au démarrage termine).
pub fn move_folder(
    catalog: &mut Catalog,
    id: FolderId,
    destination_id: FolderId,
) -> Result<MoveOutcome, CatalogError> {
    let Some(check) = check_move(catalog.connection(), id, destination_id)? else {
        return Ok(MoveOutcome::Unknown);
    };
    let target = match check.target {
        Ok(target) => target,
        Err(refusal) => return Ok(MoveOutcome::Refused(refusal)),
    };
    let folder = check.contents.folder;
    let lineage = match folder.parent_id {
        Some(parent) => folders::lineage_of(catalog.connection(), &[parent])?,
        None => Vec::new(),
    };
    let moved = FolderMove {
        folder: folder.id,
        from: folder.path,
        to: target,
        old_parent: folder.parent_id,
        new_parent: Some(destination_id),
        renamed: false,
        lineage,
        media_count: check.contents.media.len(),
    };
    match moved.redo(catalog) {
        Ok(()) => Ok(MoveOutcome::Moved(moved)),
        Err(FolderMoveError::Catalog(error)) => Err(error),
        Err(FolderMoveError::Disk(reason)) => Ok(MoveOutcome::Failed(reason)),
    }
}

/// Raison pour laquelle un dossier ne peut pas être renommé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameRefusal {
    /// Nom vide, avec « / », ou « . » et « .. ».
    InvalidName,
    /// Un fichier ou un dossier porte déjà ce nom à cet endroit.
    NameTaken,
    /// Le dossier est introuvable (disque débranché, dossier déplacé).
    Missing,
    /// Le dossier est sur une carte mémoire.
    CameraCard,
}

/// Résultat d'un renommage de dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameOutcome {
    /// Renommé.
    Renamed(FolderMove),
    /// Refusé avant de rien faire.
    Refused(RenameRefusal),
    /// Le dossier n'est plus au catalogue.
    Unknown,
    /// Le système a refusé : le dossier a gardé son nom.
    Failed(String),
}

/// Renomme le dossier `id`, de façon annulable
/// comme un déplacement.
///
/// # Errors
///
/// Échoue si le catalogue est illisible ou ne peut pas être écrit.
pub fn rename_folder(
    catalog: &mut Catalog,
    id: FolderId,
    name: &str,
) -> Result<RenameOutcome, CatalogError> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Ok(RenameOutcome::Refused(RenameRefusal::InvalidName));
    }
    let Some(contents) = folder_contents(catalog.connection(), id)? else {
        return Ok(RenameOutcome::Unknown);
    };
    let folder = contents.folder;
    let Some(parent_dir) = folder.path.parent() else {
        return Ok(RenameOutcome::Refused(RenameRefusal::InvalidName));
    };
    if !folder.path.is_dir() {
        return Ok(RenameOutcome::Refused(RenameRefusal::Missing));
    }
    if platform::is_on_camera_card(&folder.path) {
        return Ok(RenameOutcome::Refused(RenameRefusal::CameraCard));
    }
    let target = parent_dir.join(name);
    let same_except_case = target
        .to_string_lossy()
        .eq_ignore_ascii_case(&folder.path.to_string_lossy());
    if target == folder.path
        || (!same_except_case && target.symlink_metadata().is_ok())
        || folders::path_in_use(catalog.connection(), &target)?
    {
        return Ok(RenameOutcome::Refused(RenameRefusal::NameTaken));
    }
    let lineage = match folder.parent_id {
        Some(parent) => folders::lineage_of(catalog.connection(), &[parent])?,
        None => Vec::new(),
    };
    let renamed = FolderMove {
        folder: folder.id,
        from: folder.path,
        to: target,
        old_parent: folder.parent_id,
        new_parent: folder.parent_id,
        renamed: true,
        lineage,
        media_count: contents.media.len(),
    };
    match renamed.redo(catalog) {
        Ok(()) => Ok(RenameOutcome::Renamed(renamed)),
        Err(FolderMoveError::Catalog(error)) => Err(error),
        Err(FolderMoveError::Disk(reason)) => Ok(RenameOutcome::Failed(reason)),
    }
}

/// Déplace le dossier de `from` en `to` sur le disque, puis au catalogue,
/// sous `parent`, avec son journal d'intention.
fn relocate(
    catalog: &mut Catalog,
    moved: &FolderMove,
    from: &Path,
    to: &Path,
    parent: Option<FolderId>,
) -> Result<(), FolderMoveError> {
    if folders::path_in_use(catalog.connection(), to)? {
        return Err(FolderMoveError::Disk(format!(
            "{} is already in the catalog",
            to.display()
        )));
    }
    let connection = catalog.connection();
    let operation =
        file_operations::start(connection, OperationKind::Move, None, Timestamp::now())?;
    let item = file_operations::record_pending(connection, operation, from, Some(to))?;
    if let Err(error) = move_on_disk(from, to) {
        let reason = error.to_string();
        file_operations::complete(connection, item, None, ItemResult::Failed, Some(&reason))?;
        file_operations::finish(
            connection,
            operation,
            OperationStatus::Failed,
            Timestamp::now(),
        )?;
        return Err(FolderMoveError::Disk(reason));
    }

    let transaction = catalog.transaction()?;
    file_operations::complete(&transaction, item, None, ItemResult::Done, None)?;
    if parent == moved.old_parent {
        // Retour à sa place : l'ancien parent a pu quitter le catalogue.
        folders::restore_missing(&transaction, &moved.lineage)?;
    }
    folders::relocate(&transaction, moved.folder, to, parent)?;
    folders::remove_without_media(&transaction)?;
    file_operations::finish(
        &transaction,
        operation,
        OperationStatus::Completed,
        Timestamp::now(),
    )?;
    transaction.commit().map_err(CatalogError::from)?;
    Ok(())
}
