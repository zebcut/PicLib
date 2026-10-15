//! Supprimer un dossier depuis le panneau Dossiers : ce qu'il contient, et si la corbeille est possible. Elle ne l'est
//! que si tous ses fichiers, sous-dossiers compris, sont connus du
//! catalogue : ses photos et vidéos, et leurs fichiers annexes (`.xmp`,
//! `.dop`). Les fichiers système de macOS ne comptent pas.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::folders::{self, BranchMedia};
use crate::domain::{Folder, FolderId, MediaKind};
use crate::files::FolderInventory;

/// Nombre de fichiers inconnus cités pour expliquer un refus.
pub const MAX_UNKNOWN_EXAMPLES: usize = 5;

/// Dossier à supprimer, tel que le catalogue le connaît.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderContents {
    /// Le dossier.
    pub folder: Folder,
    /// Ses sous-dossiers au catalogue, à tous les niveaux.
    pub subfolders: u64,
    /// Ses médias et ceux de ses sous-dossiers.
    pub media: Vec<BranchMedia>,
}

impl FolderContents {
    /// Nombre de photos.
    #[must_use]
    pub fn photos(&self) -> u64 {
        self.count(MediaKind::Photo)
    }

    /// Nombre de vidéos.
    #[must_use]
    pub fn videos(&self) -> u64 {
        self.count(MediaKind::Video)
    }

    /// Taille totale des médias, en octets.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.media.iter().map(|media| media.file_size).sum()
    }

    fn count(&self, kind: MediaKind) -> u64 {
        self.media.iter().filter(|media| media.kind == kind).count() as u64
    }
}

/// Mise à la corbeille possible, ou pourquoi elle ne l'est pas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashCheck {
    /// Tous les fichiers sont connus du catalogue.
    Possible,
    /// Le dossier est introuvable (disque débranché, dossier déplacé).
    Offline,
    /// Un dossier n'a pas pu être lu : on ne peut pas savoir ce qu'il
    /// contient.
    Unreadable,
    /// Des fichiers sont inconnus du catalogue : les premiers, et leur
    /// nombre.
    UnknownFiles {
        /// Au plus [`MAX_UNKNOWN_EXAMPLES`] fichiers.
        examples: Vec<PathBuf>,
        /// Nombre total.
        count: u64,
    },
}

/// Lit ce que le catalogue sait du dossier `id` ; `None` s'il n'y est plus.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn folder_contents(
    connection: &Connection,
    id: FolderId,
) -> Result<Option<FolderContents>, CatalogError> {
    let Some(folder) = folders::list(connection)?
        .into_iter()
        .find(|folder| folder.id == id)
    else {
        return Ok(None);
    };
    Ok(Some(FolderContents {
        subfolders: folders::branch_size(connection, id)?.saturating_sub(1),
        media: folders::media_in_branch(connection, id)?,
        folder,
    }))
}

/// Dit si le dossier peut aller à la corbeille, d'après son inventaire sur
/// le disque (`None` : dossier introuvable).
#[must_use]
pub fn trash_check(
    contents: &FolderContents,
    inventory: Option<io::Result<FolderInventory>>,
) -> TrashCheck {
    let inventory = match inventory {
        None => return TrashCheck::Offline,
        Some(Err(error)) => {
            tracing::warn!(folder = %contents.folder.path.display(), %error, "folder cannot be read");
            return TrashCheck::Unreadable;
        }
        Some(Ok(inventory)) => inventory,
    };
    let unknown = unknown_files(&inventory.files, &contents.media);
    if unknown.is_empty() {
        TrashCheck::Possible
    } else {
        TrashCheck::UnknownFiles {
            count: unknown.len() as u64,
            examples: unknown.into_iter().take(MAX_UNKNOWN_EXAMPLES).collect(),
        }
    }
}

/// Fichiers inconnus du catalogue parmi `files` : ni un média, ni le
/// fichier annexe d'un média (`DSC_0001.NEF.xmp`, `DSC_0001.xmp`,
/// `DSC_0001.NEF.dop`), en ignorant les majuscules.
#[must_use]
pub fn unknown_files(files: &[PathBuf], media: &[BranchMedia]) -> Vec<PathBuf> {
    let known: HashSet<String> = media.iter().map(|media| key(&media.path)).collect();
    // Noms et noms sans extension des médias, par dossier.
    let mut names: HashMap<String, HashSet<String>> = HashMap::new();
    for media in media {
        let (Some(folder), Some(name)) = (media.path.parent(), media.path.file_name()) else {
            continue;
        };
        let name = name.to_string_lossy().to_lowercase();
        let entry = names.entry(key(folder)).or_default();
        if let Some((stem, _)) = name.rsplit_once('.') {
            entry.insert(stem.to_owned());
        }
        entry.insert(name);
    }
    files
        .iter()
        .filter(|file| !known.contains(&key(file)) && !is_annex(file, &names))
        .cloned()
        .collect()
}

/// Fichier annexe (`.xmp`, `.dop`) d'un média du même dossier.
fn is_annex(file: &Path, names: &HashMap<String, HashSet<String>>) -> bool {
    let (Some(folder), Some(name)) = (file.parent(), file.file_name()) else {
        return false;
    };
    let name = name.to_string_lossy().to_lowercase();
    let Some((owner, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if extension != "xmp" && extension != "dop" {
        return false;
    }
    names
        .get(&key(folder))
        .is_some_and(|known| known.contains(owner))
}

/// Chemin comparé sans tenir compte des majuscules (macOS ne les distingue
/// pas par défaut).
fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}
