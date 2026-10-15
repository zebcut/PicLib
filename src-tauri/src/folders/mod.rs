//! Dossiers du catalogue : l'arborescence du panneau
//! Dossiers, tous les dossiers même vides, regroupée par disque ; ce qu'il
//! faut savoir avant de supprimer un dossier.

mod deletion;
mod panel;

pub use deletion::{
    FolderContents, MAX_UNKNOWN_EXAMPLES, TrashCheck, folder_contents, trash_check, unknown_files,
};
pub use panel::{DiskEntry, FolderEntry, FolderPanel, folder_panel};
