//! Relocaliser : dire au catalogue où se trouve
//! désormais un fichier, ou un dossier entier, déplacé hors de PicLib. Rien
//! ne bouge sur le disque : le catalogue suit. Annulable (Cmd+Z).
//!
//! Pour un dossier, ses fichiers sont retrouvés par leur nom, à la même
//! place relative ; ceux qui manquent restent notés manquants.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use crate::catalog::repositories::{folders, media, missing};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{Folder, FolderId, MediaFormat, MediaId};
use crate::edits::{
    Edit, EditSummary, FolderRelocation, History, MediaPlace, PlacementChange, PlacementEdit,
};

/// Relocalisation refusée.
#[derive(Debug, thiserror::Error)]
pub enum RelocationError {
    /// Le média, le dossier ou le fichier choisi est introuvable.
    #[error("not found")]
    NotFound,
    /// Le fichier choisi n'est pas du même type que la photo.
    #[error("the chosen file is of another format")]
    WrongFormat,
    /// Le fichier ou le dossier choisi est déjà au catalogue.
    #[error("already in the catalog")]
    AlreadyInCatalog,
    /// Le dossier choisi est le dossier lui-même ou l'un de ses
    /// sous-dossiers.
    #[error("the folder cannot be relocated into itself")]
    IntoItself,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Le média `id` se trouve désormais à `path`.
///
/// # Errors
///
/// Voir [`RelocationError`] ; rien n'est changé en cas d'échec.
pub fn relocate_file(
    catalog: &mut Catalog,
    history: &mut History,
    id: MediaId,
    path: &Path,
) -> Result<EditSummary, RelocationError> {
    let connection = catalog.connection();
    let Some((found, old_path)) = media::read_with_paths(connection, &[id])?
        .into_iter()
        .next()
    else {
        return Err(RelocationError::NotFound);
    };
    let (Some(folder), Some(file_name)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(RelocationError::NotFound);
    };
    if !path.is_file() {
        return Err(RelocationError::NotFound);
    }
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(MediaFormat::from_extension);
    if format != Some(found.data.format) {
        return Err(RelocationError::WrongFormat);
    }
    if let Some(known) = folders::find_by_path(connection, folder)?
        && let Some(other) = media::find_in_folder(connection, known.id, file_name)?
        && other.id != id
    {
        return Err(RelocationError::AlreadyInCatalog);
    }
    let change = PlacementChange {
        media: id,
        before: MediaPlace {
            folder: old_path.parent().map(Path::to_path_buf).unwrap_or_default(),
            file_name: found.data.file_name,
        },
        after: MediaPlace {
            folder: folder.to_path_buf(),
            file_name: file_name.to_owned(),
        },
        missing_before: missing::missing_ids(connection)?.contains(&id),
        missing_after: false,
        stack: None,
    };
    let edit = PlacementEdit::new(connection, vec![change])?;
    Ok(history.perform(catalog, Edit::Relocation(edit))?)
}

/// Ce que donnerait la relocalisation d'un dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRelocationPlan {
    relocation: FolderRelocation,
}

impl FolderRelocationPlan {
    /// Médias du dossier et de ses sous-dossiers.
    #[must_use]
    pub fn total(&self) -> usize {
        self.relocation.media.len()
    }

    /// Médias retrouvés au nouvel emplacement.
    #[must_use]
    pub fn found(&self) -> usize {
        self.total() - self.relocation.missing_after.len()
    }
}

/// Prépare la relocalisation du dossier `id` en `new_path`.
///
/// # Errors
///
/// Voir [`RelocationError`].
pub fn plan_folder(
    connection: &Connection,
    id: FolderId,
    new_path: &Path,
) -> Result<FolderRelocationPlan, RelocationError> {
    let all = folders::list(connection)?;
    let Some(folder) = all.iter().find(|folder| folder.id == id) else {
        return Err(RelocationError::NotFound);
    };
    if !new_path.is_dir() {
        return Err(RelocationError::NotFound);
    }
    if new_path.starts_with(&folder.path) {
        return Err(RelocationError::IntoItself);
    }
    // Un dossier du catalogue hors de la branche, à cet endroit ou dessous :
    // les chemins se mêleraient.
    let branch: HashSet<FolderId> = branch_of(&all, id);
    if all
        .iter()
        .any(|other| !branch.contains(&other.id) && other.path.starts_with(new_path))
    {
        return Err(RelocationError::AlreadyInCatalog);
    }
    let in_branch = folders::media_in_branch(connection, id)?;
    let missing_before: Vec<MediaId> = {
        let missing = missing::missing_ids(connection)?;
        in_branch
            .iter()
            .filter(|media| missing.contains(&media.id))
            .map(|media| media.id)
            .collect()
    };
    let missing_after = in_branch
        .iter()
        .filter(|media| {
            let relative = media.path.strip_prefix(&folder.path).unwrap_or(&media.path);
            !new_path.join(relative).is_file()
        })
        .map(|media| media.id)
        .collect();
    let new_parent = new_path
        .ancestors()
        .skip(1)
        .find_map(|ancestor| {
            all.iter()
                .find(|other| other.path == ancestor && !branch.contains(&other.id))
        })
        .map(|parent| parent.id);
    Ok(FolderRelocationPlan {
        relocation: FolderRelocation {
            folder: id,
            from: folder.path.clone(),
            to: new_path.to_path_buf(),
            old_parent: folder.parent_id,
            new_parent,
            media: in_branch.iter().map(|media| media.id).collect(),
            missing_before,
            missing_after,
        },
    })
}

/// Relocalise le dossier `id` en `new_path`, de façon annulable.
///
/// # Errors
///
/// Voir [`RelocationError`] ; rien n'est changé en cas d'échec.
pub fn relocate_folder(
    catalog: &mut Catalog,
    history: &mut History,
    id: FolderId,
    new_path: &Path,
) -> Result<EditSummary, RelocationError> {
    let plan = plan_folder(catalog.connection(), id, new_path)?;
    Ok(history.perform(catalog, Edit::FolderRelocation(plan.relocation))?)
}

/// Le dossier `id` et tous ses sous-dossiers.
fn branch_of(all: &[Folder], id: FolderId) -> HashSet<FolderId> {
    let mut branch = HashSet::from([id]);
    loop {
        let before = branch.len();
        for folder in all {
            if folder
                .parent_id
                .is_some_and(|parent| branch.contains(&parent))
            {
                branch.insert(folder.id);
            }
        }
        if branch.len() == before {
            return branch;
        }
    }
}
