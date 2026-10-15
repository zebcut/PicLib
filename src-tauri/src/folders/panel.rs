//! Contenu du panneau Dossiers : tous les dossiers du
//! catalogue, même vides, avec le nombre de photos de leur branche,
//! regroupés par disque, branché ou non.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::folders;
use crate::domain::{Folder, FolderId};
use crate::platform::Disk;

/// Disque du panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskEntry {
    /// Nom du disque ; `None` pour le disque de démarrage de nom inconnu.
    pub name: Option<String>,
    /// Racine du disque.
    pub root: PathBuf,
    /// Branché ou non.
    pub online: bool,
    /// Photos et vidéos du catalogue sur ce disque.
    pub media_count: u64,
}

/// Dossier du panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    /// Identifiant du dossier.
    pub id: FolderId,
    /// Dossier parent au catalogue ; `None` pour un dossier du premier
    /// niveau de son disque.
    pub parent_id: Option<FolderId>,
    /// Rang de son disque dans [`FolderPanel::disks`].
    pub disk: usize,
    /// Nom affiché (dernier élément du chemin).
    pub name: String,
    /// Chemin complet.
    pub path: PathBuf,
    /// Photos et vidéos du dossier et de ses sous-dossiers.
    pub media_count: u64,
}

/// Contenu du panneau Dossiers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderPanel {
    /// Disques, le disque de démarrage d'abord, puis par nom.
    pub disks: Vec<DiskEntry>,
    /// Tous les dossiers du catalogue, même sans photo.
    pub folders: Vec<FolderEntry>,
}

/// Lit le panneau Dossiers. `disk_of` donne le disque d'un chemin,
/// `is_online` dit s'il est branché ; ils ne sont appelés que pour les
/// dossiers du premier niveau et pour chaque disque.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn folder_panel(
    connection: &Connection,
    startup: &Disk,
    disk_of: impl Fn(&Path) -> Disk,
    is_online: impl Fn(&Disk) -> bool,
) -> Result<FolderPanel, CatalogError> {
    let all = folders::list(connection)?;
    let branch_counts = branch_counts(&all, folders::media_counts(connection)?);
    let by_id: HashMap<FolderId, &Folder> = all.iter().map(|folder| (folder.id, folder)).collect();

    // Chaque dossier, avec le disque de son dossier du premier niveau.
    let mut root_disks: HashMap<FolderId, Disk> = HashMap::new();
    let mut disk_counts: HashMap<Disk, u64> = HashMap::new();
    let mut kept: Vec<(FolderEntry, Disk)> = Vec::new();
    for folder in &all {
        let media_count = branch_counts.get(&folder.id).copied().unwrap_or(0);
        // Un parent absent (base abîmée) : le dossier passe au premier
        // niveau.
        let parent_id = folder.parent_id.filter(|id| by_id.contains_key(id));
        let root = root_of(folder, &by_id);
        let disk = root_disks
            .entry(root.id)
            .or_insert_with(|| disk_of(&root.path))
            .clone();
        if parent_id.is_none() {
            *disk_counts.entry(disk.clone()).or_insert(0) += media_count;
        }
        let entry = FolderEntry {
            id: folder.id,
            parent_id,
            disk: 0,
            name: folder.path.file_name().map_or_else(
                || folder.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            path: folder.path.clone(),
            media_count,
        };
        kept.push((entry, disk));
    }

    // Le disque de démarrage d'abord, puis les autres par nom.
    let mut disks: Vec<(Disk, u64)> = disk_counts.into_iter().collect();
    disks.sort_by(|(a, _), (b, _)| {
        (a != startup, &a.name, &a.root).cmp(&(b != startup, &b.name, &b.root))
    });
    let rank: HashMap<&Disk, usize> = disks
        .iter()
        .enumerate()
        .map(|(rank, (disk, _))| (disk, rank))
        .collect();
    let folders = kept
        .iter()
        .map(|(entry, disk)| FolderEntry {
            disk: rank.get(disk).copied().unwrap_or(0),
            ..entry.clone()
        })
        .collect();
    let disks = disks
        .iter()
        .map(|(disk, media_count)| DiskEntry {
            name: disk.name.clone(),
            root: disk.root.clone(),
            online: is_online(disk),
            media_count: *media_count,
        })
        .collect();
    Ok(FolderPanel { disks, folders })
}

/// Premier ancêtre au catalogue d'un dossier (lui-même s'il est au premier
/// niveau), protégé contre une boucle dans une base abîmée.
fn root_of<'a>(folder: &'a Folder, by_id: &HashMap<FolderId, &'a Folder>) -> &'a Folder {
    let mut current = folder;
    for _ in 0..by_id.len() {
        match current.parent_id.and_then(|id| by_id.get(&id)) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    current
}

/// Nombre de médias de la branche de chaque dossier : les siens et ceux de
/// ses sous-dossiers. Les dossiers sans média dans leur branche sont
/// absents (zéro).
fn branch_counts(all: &[Folder], direct: Vec<(FolderId, u64)>) -> HashMap<FolderId, u64> {
    let parents: HashMap<FolderId, Option<FolderId>> = all
        .iter()
        .map(|folder| (folder.id, folder.parent_id))
        .collect();
    let mut counts: HashMap<FolderId, u64> = HashMap::new();
    for (id, count) in direct {
        let mut current = Some(id);
        // Au plus autant d'étapes que de dossiers : protège d'une boucle.
        for _ in 0..=parents.len() {
            let Some(folder) = current.filter(|folder| parents.contains_key(folder)) else {
                break;
            };
            *counts.entry(folder).or_insert(0) += count;
            current = parents[&folder];
        }
    }
    counts
}
