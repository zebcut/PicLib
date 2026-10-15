//! Écriture des fichiers annexes XMP à côté des photos.
//!
//! Le fichier annexe est le seul fichier de l'utilisateur que PicLib
//! modifie, et seulement pour ses propres propriétés (la fusion de
//! `metadata::xmp` garde tout le reste). L'écriture ne laisse jamais un
//! fichier à moitié écrit : le nouveau contenu est écrit sous un nom
//! provisoire caché, relu et comparé, puis prend la place de l'ancien d'un
//! seul coup (renommage). Un fichier modifié entre la lecture et l'écriture
//! (par un autre logiciel) n'est pas remplacé.
//!
//! Avant de remplacer un fichier que PicLib n'a pas écrit lui-même, une
//! copie vérifiée en est gardée dans le dossier de travail
//! (`keep_original_sidecar`).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::copy::link_without_overwrite;
use super::unique_path;
use crate::domain::Timestamp;

/// Date de modification et taille d'un fichier : ce que PicLib retient d'un
/// fichier annexe pour savoir, plus tard, si un autre logiciel l'a modifié.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    /// Date de modification.
    pub modified: Timestamp,
    /// Taille, en octets.
    pub size: u64,
}

/// Écriture refusée.
#[derive(Debug, thiserror::Error)]
pub enum SidecarWriteError {
    /// Le fichier a changé (ou est apparu) depuis sa lecture : il n'est pas
    /// remplacé.
    #[error("the sidecar changed since it was read")]
    ChangedMeanwhile,
    /// Le contenu relu ne correspond pas à celui écrit.
    #[error("written sidecar does not match")]
    Mismatch,
    /// Erreur du système de fichiers (droits, disque plein, débranché...).
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// État actuel d'un fichier ; `None` s'il n'existe pas.
///
/// # Errors
///
/// Échoue si le fichier existe mais ne peut pas être examiné.
pub fn file_stamp(path: &Path) -> io::Result<Option<FileStamp>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileStamp {
            modified: Timestamp::from_system_time(metadata.modified()?),
            size: metadata.len(),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Écrit `bytes` sous `target`. `expected` : l'état du fichier quand il a
/// été lu (`None` : il n'existait pas) ; s'il a changé depuis, rien n'est
/// écrit. Renvoie l'état du fichier écrit.
///
/// # Errors
///
/// Voir [`SidecarWriteError`] ; en cas d'échec, `target` n'est pas modifié
/// et le fichier provisoire est retiré.
pub fn write_sidecar(
    target: &Path,
    bytes: &[u8],
    expected: Option<FileStamp>,
) -> Result<FileStamp, SidecarWriteError> {
    let temporary = temporary_path(target)?;
    let result = write_verified(&temporary, bytes).and_then(|()| {
        if file_stamp(target)? != expected {
            return Err(SidecarWriteError::ChangedMeanwhile);
        }
        put_in_place(&temporary, target, expected.is_some())?;
        file_stamp(target)?.ok_or(SidecarWriteError::Mismatch)
    });
    // Le fichier provisoire est le nôtre, créé juste au-dessus : l'effacer
    // ne touche à rien qui appartienne à l'utilisateur.
    if temporary.symlink_metadata().is_ok()
        && let Err(error) = fs::remove_file(&temporary)
    {
        tracing::warn!(path = %temporary.display(), %error, "cannot remove a temporary file");
    }
    result
}

/// Garde une copie vérifiée du fichier annexe `source` dans `backup_dir`,
/// avant que PicLib ne le modifie ; renvoie le chemin de la copie.
///
/// # Errors
///
/// Échoue si la copie ne peut pas être écrite ou ne correspond pas.
pub fn keep_original_sidecar(source: &Path, backup_dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(backup_dir)?;
    let name = source
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let (stem, extension) = name
        .rsplit_once('.')
        .map_or((name.as_str(), "xmp"), |(stem, extension)| {
            (stem, extension)
        });
    let bytes = fs::read(source)?;
    let copy = unique_path(backup_dir, stem, extension);
    match write_verified(&copy, &bytes) {
        Ok(()) => Ok(copy),
        Err(SidecarWriteError::Io(error)) => Err(error),
        Err(error) => Err(io::Error::other(error.to_string())),
    }
}

/// Nom provisoire libre, caché, dans le dossier de `target`.
fn temporary_path(target: &Path) -> io::Result<PathBuf> {
    let dir = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
    let name = target
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Ok(unique_path(dir, &format!(".{name}.piclib"), "tmp"))
}

/// Écrit dans un fichier neuf (jamais un fichier existant), puis le relit.
fn write_verified(path: &Path, bytes: &[u8]) -> Result<(), SidecarWriteError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if fs::read(path)? == bytes {
        Ok(())
    } else {
        Err(SidecarWriteError::Mismatch)
    }
}

/// Donne au fichier provisoire le nom de `target`. Un fichier neuf ne
/// remplace jamais un fichier apparu entre-temps (voir
/// [`link_without_overwrite`]) ; un fichier existant est remplacé d'un coup
/// par renommage.
fn put_in_place(temporary: &Path, target: &Path, replaces: bool) -> Result<(), SidecarWriteError> {
    if replaces {
        fs::rename(temporary, target)?;
        return Ok(());
    }
    link_without_overwrite(temporary, target).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            SidecarWriteError::ChangedMeanwhile
        } else {
            error.into()
        }
    })
}
