//! Déplacement d'un dossier ou d'un fichier sur un même volume : un simple
//! renommage pour le système, instantané, qui ne remplace jamais ce qui
//! existe.

use std::io;
use std::path::{Path, PathBuf};

use crate::platform;

/// Premier chemin libre dans `dir` parmi `name`, `name-2`, `name-3`… : ni
/// sur le disque, ni pris selon `is_taken` (un dossier du catalogue).
/// Le choix reste indicatif : [`move_folder`] refuse lui-même d'écraser.
#[must_use]
pub fn free_folder_path(dir: &Path, name: &str, is_taken: impl Fn(&Path) -> bool) -> PathBuf {
    let is_free = |candidate: &Path| candidate.symlink_metadata().is_err() && !is_taken(candidate);
    let first = dir.join(name);
    if is_free(&first) {
        return first;
    }
    (2..=u32::MAX)
        .map(|suffix| dir.join(format!("{name}-{suffix}")))
        .find(|candidate| is_free(candidate))
        .unwrap_or(first)
}

/// Déplace le dossier `from` en `to`, sur le même volume, sans jamais
/// remplacer ce qui existe à `to`.
///
/// # Errors
///
/// `ErrorKind::AlreadyExists` si `to` existe ; sinon l'erreur du système.
/// Le dossier reste alors à sa place.
pub fn move_folder(from: &Path, to: &Path) -> io::Result<()> {
    platform::rename_exclusive(from, to)?;
    tracing::info!(from = %from.display(), to = %to.display(), "folder moved");
    Ok(())
}

/// Crée le dossier `name` dans `dir` (`name-2`, `name-3`… si le nom est
/// pris : rien n'est jamais remplacé) ; renvoie son chemin.
///
/// # Errors
///
/// L'erreur du système si le dossier ne peut pas être créé.
pub fn create_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let path = free_folder_path(dir, name, |_| false);
    std::fs::create_dir(&path)?;
    tracing::info!(path = %path.display(), "folder created");
    Ok(path)
}

/// Déplace ou renomme le fichier `from` en `to`, sur le même volume, sans
/// jamais remplacer ce qui existe à `to`.
///
/// # Errors
///
/// `ErrorKind::AlreadyExists` si `to` existe ; sinon l'erreur du système.
/// Le fichier reste alors à sa place.
pub fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    platform::rename_exclusive(from, to)?;
    tracing::debug!(from = %from.display(), to = %to.display(), "file moved");
    Ok(())
}
