//! Lecture et écriture de petits fichiers texte choisis par l'utilisateur
//!(liste des mots-clés).
//!
//! L'écriture ne détruit jamais rien(4 et 5) : le
//! texte est écrit sous un nom provisoire, relu et comparé ; un fichier
//! déjà présent sous le nom choisi part à la corbeille, jamais écrasé.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::copy::link_without_overwrite;
use super::{Trash, TrashOutcome, trash_file, unique_path};

/// Lecture ou écriture refusée.
#[derive(Debug, thiserror::Error)]
pub enum TextFileError {
    /// Le fichier dépasse la taille permise.
    #[error("file larger than {max_bytes} bytes")]
    TooLarge {
        /// Taille permise, en octets.
        max_bytes: u64,
    },
    /// Le fichier n'est pas du texte UTF-8.
    #[error("file is not UTF-8 text")]
    NotUtf8,
    /// Le fichier déjà présent n'a pas pu être mis à la corbeille : rien
    /// n'a été écrit à sa place.
    #[error("cannot move the existing file to the trash: {0}")]
    CannotReplace(String),
    /// Le texte relu ne correspond pas à celui écrit.
    #[error("written file does not match")]
    Mismatch,
    /// Erreur du système de fichiers.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Lit un fichier texte UTF-8 de `max_bytes` octets au plus.
///
/// # Errors
///
/// Voir [`TextFileError`].
pub fn read_text_file(path: &Path, max_bytes: u64) -> Result<String, TextFileError> {
    if fs::metadata(path)?.len() > max_bytes {
        return Err(TextFileError::TooLarge { max_bytes });
    }
    String::from_utf8(fs::read(path)?).map_err(|_| TextFileError::NotUtf8)
}

/// Écrit `contents` sous `target`. Un fichier déjà présent à cet endroit
/// (l'utilisateur a accepté de le remplacer) est d'abord mis à la
/// corbeille. Renvoie vrai si un fichier a été remplacé.
///
/// # Errors
///
/// Voir [`TextFileError`] ; en cas d'échec, `target` n'est pas modifié et le
/// fichier provisoire est retiré.
pub fn write_text_file(
    trash: &dyn Trash,
    target: &Path,
    contents: &str,
) -> Result<bool, TextFileError> {
    let temporary = temporary_path(target)?;
    let result = write_verified(&temporary, contents.as_bytes())
        .and_then(|()| put_in_place(trash, &temporary, target));
    // Le fichier provisoire est le nôtre, créé juste au-dessus : l'effacer
    // ne touche à rien qui appartienne à l'utilisateur.
    if temporary.exists()
        && let Err(error) = fs::remove_file(&temporary)
    {
        tracing::warn!(path = %temporary.display(), %error, "cannot remove a temporary file");
    }
    result
}

/// Nom provisoire libre, caché, dans le dossier de `target`.
fn temporary_path(target: &Path) -> Result<PathBuf, TextFileError> {
    let dir = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
    let name = target
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Ok(unique_path(dir, &format!(".{name}.piclib"), "tmp"))
}

/// Écrit dans un fichier neuf (jamais un fichier existant), puis le relit.
fn write_verified(path: &Path, bytes: &[u8]) -> Result<(), TextFileError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if fs::read(path)? == bytes {
        Ok(())
    } else {
        Err(TextFileError::Mismatch)
    }
}

/// Donne au fichier provisoire le nom choisi, après avoir mis à la
/// corbeille le fichier qui le portait.
fn put_in_place(trash: &dyn Trash, temporary: &Path, target: &Path) -> Result<bool, TextFileError> {
    let replaced = match trash_file(trash, target) {
        TrashOutcome::Trashed(_) => true,
        TrashOutcome::Missing => false,
        TrashOutcome::Failed(reason) => return Err(TextFileError::CannotReplace(reason)),
    };
    // Jamais par-dessus un fichier apparu entre-temps.
    link_without_overwrite(temporary, target)?;
    Ok(replaced)
}
