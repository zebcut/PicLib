//! Import par copie : où ranger chaque fichier,
//! quels fichiers vont ensemble, et contrôles de la destination.
//!
//! Les fichiers sont rangés selon le rangement choisi (par défaut par date
//! de prise de vue, dans `destination/AAAA/MM/JJ`, voir `layout`). Les
//! fichiers d'un même nom dans un même dossier (une paire RAW + JPEG)
//! forment un **groupe** : ils vont dans le même dossier et gardent un nom
//! commun, avec leurs fichiers annexes.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use super::layout::Arrangement;
use crate::domain::{CaptureTime, CaptureTimeSource, duplicate_name_key};
use crate::files::ScannedFile;
use crate::platform;
use crate::renaming::RenameTemplate;

/// Réglages d'un import par copie.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyOptions {
    /// Dossier racine où les fichiers sont rangés.
    pub destination: PathBuf,
    /// Rangement dans la destination.
    pub arrangement: Arrangement,
    /// Renommer les copies d'après ce modèle ; `None` : elles
    /// gardent le nom de la source.
    pub rename: Option<RenameTemplate>,
    /// Copier aussi les originaux, intacts, dans ce dossier :
    /// sous `Importé le AAAA-MM-JJ HHMM`, avec leurs noms et l'arborescence
    /// de la source. Ces copies n'entrent pas au catalogue.
    pub backup: Option<PathBuf>,
    /// Ajouter le chemin d'origine de chaque fichier copié (dossier et nom)
    /// à sa légende, à la suite d'une légende existante.
    pub original_path_in_caption: bool,
}

/// Destination refusée avant de rien copier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DestinationRefusal {
    /// La destination est dans le dossier importé, ou l'inverse : les
    /// copies pourraient se retrouver dans la source.
    #[error("{} and {} overlap", source_root.display(), destination.display())]
    Overlap {
        /// Dossier importé.
        source_root: PathBuf,
        /// Destination choisie.
        destination: PathBuf,
    },
    /// La destination est sur une carte mémoire ou un appareil photo.
    #[error("{} is on a camera card", destination.display())]
    OnCameraCard {
        /// Destination choisie.
        destination: PathBuf,
    },
}

/// Vérifie que `destination` peut recevoir les copies de `source_root`.
///
/// # Errors
///
/// Voir [`DestinationRefusal`].
pub fn check_destination(source_root: &Path, destination: &Path) -> Result<(), DestinationRefusal> {
    let source = resolved(source_root);
    let target = resolved(destination);
    if target.starts_with(&source) || source.starts_with(&target) {
        return Err(DestinationRefusal::Overlap {
            source_root: source_root.to_path_buf(),
            destination: destination.to_path_buf(),
        });
    }
    let existing = nearest_existing(&target);
    if platform::is_on_camera_card(&existing) {
        return Err(DestinationRefusal::OnCameraCard {
            destination: destination.to_path_buf(),
        });
    }
    Ok(())
}

/// Chemin absolu, liens symboliques résolus pour la partie qui existe déjà
/// (la destination peut ne pas exister encore).
fn resolved(path: &Path) -> PathBuf {
    let existing = nearest_existing(path);
    let rest = path.strip_prefix(&existing).unwrap_or(Path::new(""));
    let base = std::fs::canonicalize(&existing).unwrap_or(existing);
    rest.components().fold(base, |mut resolved, component| {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => resolved.push(name),
            _ => {}
        }
        resolved
    })
}

/// `path` lui-même s'il existe, sinon son plus proche parent existant.
fn nearest_existing(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or(path)
        .to_path_buf()
}

/// Date qui range un groupe : la première date lue dans les métadonnées
/// (un JPEG sans EXIF suit son RAW), à défaut celle du premier fichier.
#[must_use]
pub fn group_capture_time(capture_times: &[CaptureTime]) -> Option<CaptureTime> {
    capture_times
        .iter()
        .find(|capture_time| capture_time.source != CaptureTimeSource::FileModified)
        .or_else(|| capture_times.first())
        .copied()
}

/// Découpe les fichiers en lots d'environ `batch_size` fichiers, sans
/// jamais séparer un groupe (même dossier, même nom sans extension).
#[must_use]
pub fn grouped_batches(files: &[ScannedFile], batch_size: usize) -> Vec<Vec<&ScannedFile>> {
    let mut order: Vec<Vec<&ScannedFile>> = Vec::new();
    let mut positions: HashMap<(PathBuf, String), usize> = HashMap::new();
    for file in files {
        match group_key(&file.path) {
            Some(key) => {
                let position = *positions.entry(key).or_insert_with(|| {
                    order.push(Vec::new());
                    order.len() - 1
                });
                order[position].push(file);
            }
            None => order.push(vec![file]),
        }
    }

    let mut batches: Vec<Vec<&ScannedFile>> = Vec::new();
    let mut current: Vec<&ScannedFile> = Vec::new();
    for group in order {
        if !current.is_empty() && current.len() + group.len() > batch_size.max(1) {
            batches.push(std::mem::take(&mut current));
        }
        current.extend(group);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Clé du groupe d'un fichier : son dossier et son nom sans extension,
/// sans tenir compte des majuscules ni de la forme des accents.
#[must_use]
pub fn group_key(path: &Path) -> Option<(PathBuf, String)> {
    let stem = path.file_stem()?.to_str()?;
    Some((path.parent()?.to_path_buf(), duplicate_name_key(stem)))
}

/// Ce qui suit `stem` dans `file_name` (`.NEF`, `.NEF.xmp`), si `file_name`
/// commence bien par `stem` (sans tenir compte des majuscules ni de la
/// forme des accents) ; le nom commun d'un groupe y est ensuite accolé.
#[must_use]
pub fn ending_after(file_name: &str, stem: &str) -> Option<String> {
    let stem_key = duplicate_name_key(stem);
    file_name
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(file_name.len()))
        .find(|&index| duplicate_name_key(&file_name[..index]) == stem_key)
        .map(|index| file_name[index..].to_owned())
        .filter(|ending| ending.is_empty() || ending.starts_with('.'))
}
