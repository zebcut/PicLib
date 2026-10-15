//! Ajout des versions traitées au catalogue : le fichier est importé par référence, prend toutes les
//! métadonnées de son original, et rejoint sa pile, en couverture.
//!
//! Comme un import, l'ajout ne s'annule pas (la version se supprime comme
//! toute photo).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::tz::TimeZone;
use rusqlite::Connection;

use crate::catalog::repositories::edited_versions::{self, EditedVersion};
use crate::catalog::repositories::stacks::{self, Stack};
use crate::catalog::repositories::{folders, media};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::MediaId;
use crate::files::SourceScan;
use crate::import::{ImportError, ImportOptions, import_by_reference};
use crate::jobs::CancellationToken;

/// Fichiers en train d'être ajoutés comme versions : la surveillance des
/// dossiers les ignore (une copie `-Modifié` apparaît avant d'être au
/// catalogue).
static IN_PROGRESS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Vrai si `path` est en train d'être ajouté comme version.
#[must_use]
pub fn is_in_progress(path: &Path) -> bool {
    IN_PROGRESS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(path)
}

/// Note des fichiers en cours d'ajout, jusqu'à ce que la garde soit
/// abandonnée.
#[must_use]
pub fn mark_in_progress(paths: &[PathBuf]) -> InProgress {
    IN_PROGRESS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .extend(paths.iter().cloned());
    InProgress(paths.to_vec())
}

/// Garde de [`mark_in_progress`].
#[derive(Debug)]
pub struct InProgress(Vec<PathBuf>);

impl Drop for InProgress {
    fn drop(&mut self) {
        let mut set = IN_PROGRESS.lock().unwrap_or_else(PoisonError::into_inner);
        for path in &self.0 {
            set.remove(path);
        }
    }
}

/// Fichier à ajouter comme version traitée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVersion {
    /// Fichier produit par l'éditeur (ou copie `-Modifié`).
    pub path: PathBuf,
    /// Original.
    pub original_id: MediaId,
    /// Éditeur qui l'a produit ; `None` : inconnu.
    pub editor: Option<String>,
    /// Fichier de travail que « Modifier de nouveau » rouvre : un média du
    /// catalogue ; `None` : la version elle-même (une copie `-Modifié`).
    pub working_id: Option<MediaId>,
    /// Empreinte d'une copie `-Modifié` à sa création.
    pub copy_digest: Option<String>,
}

/// Ajoute les fichiers `versions` au catalogue et renvoie les médias créés,
/// dans l'ordre de `versions` (ceux qui n'ont pas pu l'être sont absents :
/// fichier disparu ou illisible). Les dossiers `excluded` (internes au
/// catalogue) ne sont jamais parcourus.
///
/// # Errors
///
/// Voir [`ImportError`] : les versions des dossiers déjà traités restent
/// au catalogue.
pub fn add_versions(
    catalog: &Mutex<Catalog>,
    versions: &[NewVersion],
    excluded: &[PathBuf],
) -> Result<Vec<(MediaId, NewVersion)>, ImportError> {
    let mut by_folder: BTreeMap<PathBuf, Vec<&NewVersion>> = BTreeMap::new();
    for version in versions {
        if let Some(folder) = version.path.parent() {
            by_folder
                .entry(folder.to_path_buf())
                .or_default()
                .push(version);
        }
    }
    let mut added = Vec::new();
    for (folder, versions) in by_folder {
        let options = ImportOptions {
            // Une copie `-Modifié` est identique à son original.
            skip_duplicates: false,
            stack_pairs: false,
            only: Some(
                versions
                    .iter()
                    .map(|version| version.path.clone())
                    .collect(),
            ),
            scan: SourceScan {
                include_subfolders: false,
                excluded: excluded.to_vec(),
            },
            ..ImportOptions::default()
        };
        let report = import_by_reference(
            catalog,
            &folder,
            &options,
            &TimeZone::system(),
            &CancellationToken::default(),
            |_, _| {},
        )?;
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        edited_versions::mark_import(&transaction, report.import_id)?;
        for version in versions {
            if let Some(id) = link(&transaction, &folder, version)? {
                added.push((id, version.clone()));
            }
        }
        transaction.commit().map_err(CatalogError::from)?;
    }
    Ok(added)
}

/// Lie une version importée à son original : métadonnées, lien, pile.
/// Renvoie `None` si le fichier n'a pas été importé.
fn link(
    connection: &Connection,
    folder: &Path,
    version: &NewVersion,
) -> Result<Option<MediaId>, CatalogError> {
    let Some(folder) = folders::find_by_path(connection, folder)? else {
        return Ok(None);
    };
    let Some(name) = version.path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(added) = media::find_in_folder(connection, folder.id, name)? else {
        return Ok(None);
    };
    let id = added.id;
    // L'original a pu quitter le catalogue entre-temps.
    if id == version.original_id || !media_exists(connection, version.original_id)? {
        return Ok(Some(id));
    }
    edited_versions::copy_metadata(connection, version.original_id, id)?;
    edited_versions::insert(
        connection,
        &EditedVersion {
            media_id: id,
            original_id: version.original_id,
            editor: version.editor.clone(),
            working_id: Some(version.working_id.unwrap_or(id)),
            copy_digest: version.copy_digest.clone(),
        },
    )?;
    stack_with_original(connection, id, version.original_id)?;
    Ok(Some(id))
}

fn media_exists(connection: &Connection, id: MediaId) -> Result<bool, CatalogError> {
    Ok(!media::read_with_paths(connection, &[id])?.is_empty())
}

/// Met la version `id` en couverture de la pile de son original (une pile
/// est créée, repliée, s'il n'en avait pas). Une pile est propre à un
/// dossier : une version rangée ailleurs (`darktable_exported`)
/// n'est pas empilée.
fn stack_with_original(
    connection: &Connection,
    id: MediaId,
    original_id: MediaId,
) -> Result<(), CatalogError> {
    let candidates = stacks::candidates(connection, &[id, original_id])?;
    let folders: HashSet<_> = candidates
        .iter()
        .map(|candidate| candidate.folder_id)
        .collect();
    if candidates.len() < 2 || folders.len() > 1 {
        return Ok(());
    }
    let existing = stacks::of_media(connection, &[original_id])?;
    let stack = match existing.into_iter().next() {
        Some(stack) => {
            stacks::delete(connection, stack.id)?;
            Stack {
                members: std::iter::once(id)
                    .chain(stack.members.into_iter().filter(|member| *member != id))
                    .collect(),
                ..stack
            }
        }
        None => Stack {
            id: stacks::next_id(connection)?,
            collapsed: true,
            members: vec![id, original_id],
        },
    };
    stacks::insert(connection, &stack)
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
