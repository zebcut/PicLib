//! Mise à la corbeille des fichiers de l'utilisateur : jamais
//! d'effacement définitif.
//!
//! La corbeille est vue à travers le trait [`Trash`] : l'application utilise
//! celle du système ([`SystemTrash`]), les tests une corbeille simulée, pour
//! ne jamais remplir la vraie corbeille de l'utilisateur.

use std::path::{Path, PathBuf};

use crate::platform;

/// Corbeille où placer des fichiers.
pub trait Trash: Sync {
    /// Place le fichier dans la corbeille et renvoie son nouvel emplacement,
    /// s'il est connu.
    ///
    /// # Errors
    ///
    /// Renvoie la raison du refus ; le fichier reste alors à sa place.
    fn move_to_trash(&self, path: &Path) -> Result<Option<PathBuf>, String>;
}

/// Corbeille du système (celle du Finder sur macOS).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTrash;

impl Trash for SystemTrash {
    fn move_to_trash(&self, path: &Path) -> Result<Option<PathBuf>, String> {
        platform::move_to_trash(path).map_err(|error| error.to_string())
    }
}

/// Résultat de la mise à la corbeille d'un fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashOutcome {
    /// Dans la corbeille (emplacement, s'il est connu).
    Trashed(Option<PathBuf>),
    /// Introuvable (disque débranché, fichier déplacé) : rien n'a été fait.
    Missing,
    /// Refusé par le système (raison) : le fichier est resté à sa place.
    Failed(String),
}

/// Place un fichier dans la corbeille, après avoir vérifié qu'il existe.
#[must_use]
pub fn trash_file(trash: &dyn Trash, path: &Path) -> TrashOutcome {
    // Un lien symbolique cassé existe quand même : on le vérifie sans le
    // suivre.
    if path.symlink_metadata().is_err() {
        return TrashOutcome::Missing;
    }
    match trash.move_to_trash(path) {
        Ok(location) => {
            tracing::info!(path = %path.display(), "moved to the trash");
            TrashOutcome::Trashed(location)
        }
        Err(reason) => {
            tracing::warn!(path = %path.display(), %reason, "cannot move a file to the trash");
            TrashOutcome::Failed(reason)
        }
    }
}
