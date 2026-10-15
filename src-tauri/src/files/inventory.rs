//! Inventaire complet d'un dossier, sous-dossiers compris : tous les
//! fichiers, pour savoir si un dossier ne contient que des fichiers connus
//! du catalogue avant de le placer dans la corbeille. Lecture seule.

use std::io;
use std::path::{Path, PathBuf};

/// Contenu d'un dossier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderInventory {
    /// Fichiers (et liens symboliques, jamais suivis), sous-dossiers
    /// compris, sauf les fichiers système de macOS.
    pub files: Vec<PathBuf>,
    /// Sous-dossiers, à tous les niveaux.
    pub folders: u64,
}

/// Fichiers que macOS dépose lui-même dans les dossiers : ils ne sont pas
/// ceux de l'utilisateur et partent avec le dossier.
#[must_use]
pub fn is_system_file(name: &str) -> bool {
    name == ".DS_Store" || name == ".localized" || name == "Icon\r" || name.starts_with("._")
}

/// Inventaire de `folder` et de ses sous-dossiers.
///
/// # Errors
///
/// Échoue si un dossier est illisible : l'inventaire ne serait pas complet.
pub fn folder_inventory(folder: &Path) -> io::Result<FolderInventory> {
    let mut inventory = FolderInventory::default();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                inventory.folders += 1;
                pending.push(entry.path());
            } else if !is_system_file(&entry.file_name().to_string_lossy()) {
                inventory.files.push(entry.path());
            }
        }
    }
    inventory.files.sort();
    Ok(inventory)
}
