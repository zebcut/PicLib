//! Enregistrement de l'arborescence des dossiers importés.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::folders;
use crate::domain::FolderId;

/// Retrouve ou crée, au catalogue, les dossiers de l'arborescence d'un
/// import, reliés à leur parent.
///
/// Les dossiers déjà connus sont réutilisés ; le dossier de départ est
/// rattaché à son parent si celui-ci est déjà au catalogue, et un dossier
/// importé avant son parent y est rattaché quand le parent arrive. Le panneau
/// Dossiers affichera ainsi une arborescence continue.
pub struct FolderTree {
    root: PathBuf,
    known: HashMap<PathBuf, FolderId>,
}

impl FolderTree {
    /// Arborescence de l'import du dossier `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            known: HashMap::new(),
        }
    }

    /// Oublie les dossiers retenus : à appeler au début de chaque transaction
    /// d'écriture, un dossier vide ayant pu quitter le catalogue entre-temps.
    pub fn forget(&mut self) {
        self.known.clear();
    }

    /// Identifiant du dossier `path` (situé dans le dossier de départ), créé
    /// au besoin avec ses parents.
    pub fn folder_id(
        &mut self,
        connection: &Connection,
        path: &Path,
    ) -> Result<FolderId, CatalogError> {
        if let Some(id) = self.known.get(path) {
            return Ok(*id);
        }
        // Le dossier de départ (ou, par sécurité, un chemin hors de lui) ne
        // crée pas ses parents : il se rattache seulement à un parent déjà
        // connu du catalogue.
        let parent_id = match path.parent() {
            Some(parent) if path != self.root && path.starts_with(&self.root) => {
                Some(self.folder_id(connection, parent)?)
            }
            Some(parent) => folders::find_by_path(connection, parent)?.map(|folder| folder.id),
            None => None,
        };

        let id = match folders::find_by_path(connection, path)? {
            Some(existing) => {
                if let (None, Some(parent_id)) = (existing.parent_id, parent_id) {
                    folders::set_parent(connection, existing.id, parent_id)?;
                }
                existing.id
            }
            None => folders::insert(connection, path, parent_id)?,
        };
        self.known.insert(path.to_path_buf(), id);
        Ok(id)
    }
}
