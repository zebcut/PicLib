//! Fichiers annexes d'un dossier (`.xmp`, `.dop`), repérés par leur nom sans
//! tenir compte des majuscules ni de la forme Unicode des accents. Lecture
//! seule : ce sont les opérations (corbeille, déplacement, renommage) qui
//! les font suivre leur photo.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::duplicate_name_key;

/// Extensions des fichiers annexes, en minuscules.
const SIDECAR_EXTENSIONS: [&str; 2] = ["xmp", "dop"];

/// Fichiers annexes d'un dossier, par nom (clé de [`duplicate_name_key`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderSidecars {
    by_name: HashMap<String, PathBuf>,
}

impl FolderSidecars {
    /// Lit les fichiers annexes de `folder` (sans ses sous-dossiers).
    ///
    /// # Errors
    ///
    /// Échoue si le dossier est illisible.
    pub fn read(folder: &Path) -> io::Result<Self> {
        let mut by_name = HashMap::new();
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_sidecar = Path::new(&name)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    SIDECAR_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
                });
            if is_sidecar {
                by_name.insert(duplicate_name_key(&name), entry.path());
            }
        }
        Ok(Self { by_name })
    }

    /// Fichiers annexes du média `file_name` : `photo.NEF.xmp` et
    /// `photo.NEF.dop`, qui n'appartiennent qu'à lui, et `photo.xmp` (nom de
    /// Lightroom) si `with_shared`.
    #[must_use]
    pub fn of(&self, file_name: &str, with_shared: bool) -> Vec<PathBuf> {
        let mut names: Vec<String> = SIDECAR_EXTENSIONS
            .iter()
            .map(|extension| format!("{file_name}.{extension}"))
            .collect();
        if with_shared && let Some(stem) = Path::new(file_name).file_stem() {
            names.push(format!("{}.xmp", stem.to_string_lossy()));
        }
        names
            .iter()
            .filter_map(|name| self.by_name.get(&duplicate_name_key(name)).cloned())
            .collect()
    }
}
