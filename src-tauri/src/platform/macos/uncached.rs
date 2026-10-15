//! Lecture et écriture sans passer par la mémoire cache de macOS : la
//! relecture d'une copie lit bien le disque(copie vérifiée).
//!
//! Appel direct au système (`fcntl`, via `libc`, déjà compilée pour
//! Tauri) : c'est le seul moyen de désactiver le cache pour un fichier.

// Seul appel au système de ce fichier, justifié ci-dessous.
#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

/// Demande à macOS de ne pas garder en mémoire cache le contenu lu ou écrit
/// par `file` : ce qui est écrit n'y reste pas, ce qui est relu vient donc
/// du disque.
///
/// # Errors
///
/// Renvoie l'erreur du système si le réglage est refusé.
pub fn disable_cache(file: &File) -> io::Result<()> {
    // SAFETY: `fcntl(F_NOCACHE)` ne reçoit qu'un descripteur ouvert, valide
    // tant que `file` est emprunté, et un entier : aucune mémoire n'est
    // partagée avec le système.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
