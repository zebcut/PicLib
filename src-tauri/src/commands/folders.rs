//! Panneau Dossiers : l'arborescence des dossiers du catalogue,
//! regroupée par disque, avec le nombre de photos de chaque dossier.

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::catalog_info::{CatalogReadFailure, read};
use crate::catalog::Catalog;
use crate::domain::FolderId;
use crate::folders::{DiskEntry, FolderEntry, FolderPanel, folder_panel};
use crate::platform;

/// Disque du panneau.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskDto {
    /// Nom ; `null` pour le disque de démarrage de nom inconnu.
    pub name: Option<String>,
    /// Racine du disque.
    pub root: String,
    /// Branché ou non.
    pub online: bool,
    /// Photos et vidéos du catalogue sur ce disque.
    pub media_count: u32,
}

/// Dossier du panneau.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    /// Identifiant.
    pub id: u32,
    /// Dossier parent ; `null` pour un dossier du premier niveau de son
    /// disque.
    pub parent_id: Option<u32>,
    /// Rang de son disque dans `disks`.
    pub disk: u32,
    /// Nom affiché.
    pub name: String,
    /// Chemin complet, affiché au survol.
    pub path: String,
    /// Photos et vidéos du dossier et de ses sous-dossiers.
    pub media_count: u32,
}

/// Contenu du panneau Dossiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderPanelDto {
    /// Disques, le disque de démarrage d'abord.
    pub disks: Vec<DiskDto>,
    /// Tous les dossiers du catalogue, même vides.
    pub folders: Vec<FolderDto>,
}

/// Renvoie le contenu du panneau Dossiers.
///
/// # Errors
///
/// [`CatalogReadFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_folder_panel(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<FolderPanelDto, CatalogReadFailure> {
    let startup = platform::startup_disk();
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        folder_panel(
            catalog.connection(),
            &startup,
            |path| platform::disk_of(path, &startup),
            platform::is_disk_online,
        )
    })
    .map(FolderPanelDto::from)
}

impl From<FolderPanel> for FolderPanelDto {
    fn from(panel: FolderPanel) -> Self {
        Self {
            disks: panel.disks.into_iter().map(DiskDto::from).collect(),
            folders: panel.folders.into_iter().map(FolderDto::from).collect(),
        }
    }
}

impl From<DiskEntry> for DiskDto {
    fn from(disk: DiskEntry) -> Self {
        Self {
            name: disk.name,
            root: disk.root.display().to_string(),
            online: disk.online,
            media_count: count_dto(disk.media_count),
        }
    }
}

impl From<FolderEntry> for FolderDto {
    fn from(folder: FolderEntry) -> Self {
        Self {
            id: folder_id_dto(folder.id),
            parent_id: folder.parent_id.map(folder_id_dto),
            disk: u32::try_from(folder.disk).unwrap_or(u32::MAX),
            name: folder.name,
            path: folder.path.display().to_string(),
            media_count: count_dto(folder.media_count),
        }
    }
}

fn folder_id_dto(id: FolderId) -> u32 {
    u32::try_from(id.get()).unwrap_or(u32::MAX)
}

fn count_dto(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}
