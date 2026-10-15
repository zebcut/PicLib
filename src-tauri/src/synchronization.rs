//! Synchroniser un dossier : comparer ce qui est sur
//! le disque avec ce que le catalogue en connaît, pour proposer d'importer
//! les nouveaux fichiers, de retirer les disparus et de relire les
//! modifiés. Lecture seule : les changements passent par les commandes
//! habituelles (import, retrait, lecture des métadonnées).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::catalog::repositories::folders;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{FolderId, MediaId, duplicate_name_key};
use crate::files::{ScanError, scan_folder};
use crate::jobs::CancellationToken;

/// Écart de date de modification au-delà duquel un fichier est tenu pour
/// modifié, en millisecondes (les systèmes de fichiers arrondissent).
const MODIFIED_TOLERANCE_MS: i64 = 1000;

/// Différences entre le disque et le catalogue pour un dossier et ses
/// sous-dossiers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderDifferences {
    /// Fichiers sur le disque inconnus du catalogue.
    pub new_files: Vec<PathBuf>,
    /// Médias dont le fichier a disparu.
    pub missing: Vec<(MediaId, PathBuf)>,
    /// Médias dont le fichier a changé (taille ou date).
    pub modified: Vec<(MediaId, PathBuf)>,
}

/// Échec de la comparaison.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Le dossier n'est plus au catalogue.
    #[error("folder not found")]
    NotFound,
    /// Le dossier est introuvable ou illisible sur le disque.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// Le catalogue n'a pas pu être lu.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Clé d'un fichier : son dossier et son nom, sans tenir compte des
/// majuscules ni de la forme Unicode des accents.
fn key(path: &Path) -> (Option<PathBuf>, Option<String>) {
    (
        path.parent().map(Path::to_path_buf),
        path.file_name()
            .map(|name| duplicate_name_key(&name.to_string_lossy())),
    )
}

/// Compare le dossier `id` du catalogue et son contenu sur le disque,
/// sous-dossiers compris.
///
/// # Errors
///
/// Voir [`SyncError`].
pub fn compare(
    catalog: &Mutex<Catalog>,
    id: FolderId,
    cancellation: &CancellationToken,
) -> Result<FolderDifferences, SyncError> {
    let (root, known) = {
        let catalog = lock(catalog);
        let connection = catalog.connection();
        let Some(folder) = folders::list(connection)?
            .into_iter()
            .find(|folder| folder.id == id)
        else {
            return Err(SyncError::NotFound);
        };
        (folder.path, folders::media_in_branch(connection, id)?)
    };
    // Le disque est lu sans tenir le catalogue.
    let scan = scan_folder(&root, cancellation, |_| {})?;
    let on_disk: HashMap<_, _> = scan
        .files
        .iter()
        .map(|file| (key(&file.path), file))
        .collect();
    let mut seen = HashSet::new();
    let mut differences = FolderDifferences::default();
    for media in known {
        let media_key = key(&media.path);
        match on_disk.get(&media_key) {
            None => differences.missing.push((media.id, media.path)),
            Some(file) => {
                seen.insert(media_key);
                let shifted = (file.modified.unix_millis() - media.file_modified_at.unix_millis())
                    .abs()
                    >= MODIFIED_TOLERANCE_MS;
                if file.size != media.file_size || shifted {
                    differences.modified.push((media.id, media.path));
                }
            }
        }
    }
    differences.new_files = scan
        .files
        .iter()
        .filter(|file| !seen.contains(&key(&file.path)))
        .map(|file| file.path.clone())
        .collect();
    differences.new_files.sort();
    Ok(differences)
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
