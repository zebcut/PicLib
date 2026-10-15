//! Fichiers d'un média sur le disque : le sien et ses fichiers annexes
//! (`.xmp`, `.dop`), qui le suivent toujours.
//!
//! `photo.NEF.xmp` et `photo.NEF.dop` n'appartiennent qu'à `photo.NEF`.
//! `photo.xmp` (nom de Lightroom) est partagé par les médias du même nom
//! dans le dossier : il revient au seul média de ce nom, sinon au seul RAW
//! de ce nom (Lightroom l'écrit pour le RAW d'une paire RAW + JPEG). Sans
//! propriétaire, il ne suit que si tous les médias de ce nom partent
//! ensemble.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::catalog::CatalogError;
use crate::catalog::repositories::media;
use crate::domain::{FolderId, Media, MediaId, duplicate_name_key};
use crate::files::FolderSidecars;
use rusqlite::Connection;

/// Un média et ses fichiers sur le disque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFiles {
    /// Média.
    pub media_id: MediaId,
    /// Dossier du catalogue.
    pub folder_id: FolderId,
    /// Chemin complet de son fichier.
    pub path: PathBuf,
    /// Ses fichiers annexes présents sur le disque.
    pub sidecars: Vec<PathBuf>,
}

/// Fichiers des médias `ids` (les identifiants inconnus sont ignorés), dans
/// l'ordre de `ids`. Un dossier illisible (disque débranché) ne donne aucun
/// fichier annexe.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn files_of(connection: &Connection, ids: &[MediaId]) -> Result<Vec<MediaFiles>, CatalogError> {
    let found = media::read_with_paths(connection, ids)?;
    let moving: HashSet<MediaId> = found.iter().map(|(media, _)| media.id).collect();
    let mut folders: HashMap<FolderId, (Option<FolderSidecars>, Vec<Media>)> = HashMap::new();
    for (media, path) in &found {
        if folders.contains_key(&media.data.folder_id) {
            continue;
        }
        let sidecars = path.parent().and_then(|folder| {
            FolderSidecars::read(folder)
                .inspect_err(|error| {
                    tracing::debug!(folder = %folder.display(), %error, "cannot list the sidecars");
                })
                .ok()
        });
        let siblings = media::list_in_folder(connection, media.data.folder_id)?;
        folders.insert(media.data.folder_id, (sidecars, siblings));
    }
    let order: HashMap<MediaId, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut files: Vec<MediaFiles> = found
        .into_iter()
        .map(|(media, path)| {
            let (sidecars, siblings) = &folders[&media.data.folder_id];
            let sidecars = sidecars.as_ref().map_or_else(Vec::new, |sidecars| {
                sidecars.of(
                    &media.data.file_name,
                    takes_shared(&media, siblings, &moving),
                )
            });
            MediaFiles {
                media_id: media.id,
                folder_id: media.data.folder_id,
                path,
                sidecars,
            }
        })
        .collect();
    files.sort_by_key(|file| order.get(&file.media_id).copied().unwrap_or(usize::MAX));
    Ok(files)
}

/// Vrai si `photo.xmp` suit `media` quand les médias `moving` partent.
fn takes_shared(media: &Media, siblings: &[Media], moving: &HashSet<MediaId>) -> bool {
    let stem = |name: &str| {
        Path::new(name)
            .file_stem()
            .map(|stem| duplicate_name_key(&stem.to_string_lossy()))
    };
    let own = stem(&media.data.file_name);
    let same: Vec<&Media> = siblings
        .iter()
        .filter(|sibling| stem(&sibling.data.file_name) == own)
        .collect();
    let raws: Vec<MediaId> = same
        .iter()
        .filter(|sibling| sibling.data.format.is_raw())
        .map(|sibling| sibling.id)
        .collect();
    let owner = match (same.as_slice(), raws.as_slice()) {
        ([only], _) => Some(only.id),
        (_, [raw]) => Some(*raw),
        _ => None,
    };
    match owner {
        Some(owner) => owner == media.id,
        // Sans propriétaire : avec le premier des médias de ce nom, si
        // tous partent.
        None => {
            same.iter().all(|sibling| moving.contains(&sibling.id))
                && same.iter().map(|sibling| sibling.id).min() == Some(media.id)
        }
    }
}
