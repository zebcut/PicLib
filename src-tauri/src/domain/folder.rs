//! Dossier du disque référencé dans le catalogue.

use std::path::PathBuf;

use super::FolderId;

/// Dossier contenant des médias du catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// Identifiant dans le catalogue.
    pub id: FolderId,
    /// Dossier parent dans l'arborescence du catalogue, s'il y est aussi.
    pub parent_id: Option<FolderId>,
    /// Chemin absolu du dossier sur le disque.
    pub path: PathBuf,
}
