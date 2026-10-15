//! Opérations sur le disque : parcours de dossiers en lecture seule, copie
//! vérifiée (import par copie), conflits de noms, mise à la corbeille,
//! petits fichiers texte (liste des mots-clés), déplacement et renommage
//! de dossiers et de fichiers, écriture des fichiers annexes XMP, copie
//! d'une photo à retoucher.
//!
//! Toute opération sur les fichiers de l'utilisateur passe par ce module
//!. Le journal des opérations est tenu dans le
//! catalogue (`catalog::repositories::file_operations`).

mod copy;
mod edit_copy;
mod inventory;
mod moving;
mod naming;
mod scan;
mod sidecar_write;
mod sidecars;
mod text;
mod trash;

pub use copy::{
    ContentDigest, CopyError, PendingCopy, PlaceError, copy_verified, file_digest,
    remove_partial_copies,
};
pub use edit_copy::copy_for_editing;
pub use inventory::{FolderInventory, folder_inventory, is_system_file};
pub use moving::{create_folder, free_folder_path, move_file, move_folder};
pub use naming::{unique_path, unique_stem};
pub use scan::{
    ScanError, ScanIssue, ScanReport, ScanSummary, ScannedFile, SourceScan, list_folder,
    scan_folder, scan_source,
};
pub use sidecar_write::{
    FileStamp, SidecarWriteError, file_stamp, keep_original_sidecar, write_sidecar,
};
pub use sidecars::FolderSidecars;
pub use text::{TextFileError, read_text_file, write_text_file};
pub use trash::{SystemTrash, Trash, TrashOutcome, trash_file};
