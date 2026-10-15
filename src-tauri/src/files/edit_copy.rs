//! Copie d'une photo à retoucher dans un éditeur externe : `IMG_1234-Modifié.jpg`, à côté de l'original, vérifiée
//! comme toute copie ; l'original n'est jamais touché.

use std::path::{Path, PathBuf};

use super::copy::{ContentDigest, CopyError, PlaceError, copy_verified};
use super::naming::unique_path;
use crate::jobs::CancellationToken;

/// Copie `source` dans son dossier sous le nom `<nom><suffix>.<extension>`
/// (`-2`, `-3`... si ce nom est pris : rien n'est écrasé). Renvoie le
/// chemin de la copie et l'empreinte de son contenu.
///
/// # Errors
///
/// Voir [`CopyError`] ; aucune copie ne reste alors.
pub fn copy_for_editing(
    source: &Path,
    suffix: &str,
    cancellation: &CancellationToken,
) -> Result<(PathBuf, ContentDigest), CopyError> {
    let not_a_file = || CopyError::Source {
        path: source.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a file"),
    };
    let dir = source.parent().ok_or_else(not_a_file)?;
    let stem = source
        .file_stem()
        .ok_or_else(not_a_file)?
        .to_string_lossy()
        .into_owned();
    let extension = source
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut pending = copy_verified(source, dir, cancellation)?;
    let digest = pending.digest();
    let base = format!("{stem}{suffix}");
    // Un nom libre au moment du choix peut être pris juste avant le nommage :
    // on essaie alors le suivant.
    loop {
        let target = unique_path(dir, &base, &extension);
        match pending.place(&target) {
            Ok(path) => return Ok((path, digest)),
            Err(PlaceError::Taken(returned)) => pending = returned,
            Err(PlaceError::Failed(error)) => return Err(error),
        }
    }
}
