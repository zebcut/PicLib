//! Fichiers annexes des fichiers importés : `photo.NEF.xmp`
//! appartient toujours à `photo.NEF` ; `photo.xmp` (nom de Lightroom) est
//! partagé par les fichiers du même nom, et revient à l'un d'eux ;
//! `photo.NEF.dop` (DxO PhotoLab) appartient toujours à `photo.NEF`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::domain::duplicate_name_key;
use crate::files::ScannedFile;

/// Fichiers annexes d'un import, repérés par dossier et par nom (sans
/// tenir compte des majuscules ni de la forme Unicode des accents).
#[derive(Debug, Default)]
pub struct Sidecars {
    /// Fichier annexe, par dossier et nom complet (`dsc_0001.nef.xmp`).
    by_name: HashMap<(PathBuf, String), PathBuf>,
    /// Médias importés, par dossier et nom sans extension : combien, et
    /// combien de RAW.
    siblings: HashMap<(PathBuf, String), Siblings>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Siblings {
    all: usize,
    raw: usize,
}

/// Fichier annexe qui décrit peut-être un média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarMatch<'a> {
    /// Chemin du fichier annexe.
    pub path: &'a Path,
    /// Vrai pour `photo.NEF.xmp` : il ne peut décrire que ce média.
    is_own: bool,
    /// Pour `photo.xmp` sans indication : vrai s'il revient à ce média.
    belongs_by_default: bool,
    /// Extension du média (`NEF`).
    extension: String,
}

impl SidecarMatch<'_> {
    /// Vrai si le fichier annexe décrit bien ce média. `photo.xmp` peut
    /// désigner l'extension de sa photo (`SidecarForExtension`) ; sans
    /// cette indication, il revient au RAW s'il y en a un seul de ce nom,
    /// sinon au média s'il est seul de ce nom (Lightroom l'écrit pour le
    /// RAW d'une paire RAW + JPEG).
    #[must_use]
    pub fn accepts(&self, sidecar_for_extension: Option<&str>) -> bool {
        if self.is_own {
            return true;
        }
        sidecar_for_extension.map_or(self.belongs_by_default, |extension| {
            extension.eq_ignore_ascii_case(&self.extension)
        })
    }
}

impl Sidecars {
    /// Repère les fichiers annexes `sidecars` des médias `files`.
    #[must_use]
    pub fn new(files: &[ScannedFile], sidecars: &[PathBuf]) -> Self {
        let mut index = Self::default();
        for path in sidecars {
            if let Some(key) = key(path, file_name(path)) {
                index.by_name.insert(key, path.clone());
            }
        }
        if index.by_name.is_empty() {
            return index;
        }
        for file in files {
            if let Some(key) = key(&file.path, file_stem(&file.path)) {
                let siblings = index.siblings.entry(key).or_default();
                siblings.all += 1;
                siblings.raw += usize::from(file.format.is_raw());
            }
        }
        index
    }

    /// Fichier annexe candidat pour `file` : `photo.NEF.xmp` d'abord, sinon
    /// `photo.xmp`.
    #[must_use]
    pub fn find_xmp(&self, file: &ScannedFile) -> Option<SidecarMatch<'_>> {
        let name = file_name(&file.path)?;
        let extension = file.path.extension()?.to_str()?.to_owned();
        if let Some(path) = self.lookup(&file.path, &format!("{name}.xmp")) {
            return Some(SidecarMatch {
                path,
                is_own: true,
                belongs_by_default: true,
                extension,
            });
        }
        let stem = file_stem(&file.path)?;
        let path = self.lookup(&file.path, &format!("{stem}.xmp"))?;
        let siblings = key(&file.path, Some(stem))
            .and_then(|key| self.siblings.get(&key).copied())
            .unwrap_or_default();
        let belongs_by_default = if file.format.is_raw() {
            siblings.raw == 1
        } else {
            siblings.raw == 0 && siblings.all == 1
        };
        Some(SidecarMatch {
            path,
            is_own: false,
            belongs_by_default,
            extension,
        })
    }

    /// Fichier `.dop` de DxO PhotoLab de `file` (`photo.NEF.dop`).
    #[must_use]
    pub fn find_dop(&self, file: &ScannedFile) -> Option<&Path> {
        self.lookup(&file.path, &format!("{}.dop", file_name(&file.path)?))
    }

    fn lookup(&self, media: &Path, sidecar_name: &str) -> Option<&Path> {
        let folder = media.parent()?.to_path_buf();
        self.by_name
            .get(&(folder, duplicate_name_key(sidecar_name)))
            .map(PathBuf::as_path)
    }
}

/// Clé d'un nom dans son dossier.
fn key(path: &Path, name: Option<&str>) -> Option<(PathBuf, String)> {
    Some((path.parent()?.to_path_buf(), duplicate_name_key(name?)))
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

fn file_stem(path: &Path) -> Option<&str> {
    path.file_stem()?.to_str()
}
