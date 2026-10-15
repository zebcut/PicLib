//! Renommage exclusif : `renamex_np` avec `RENAME_EXCL` refuse de remplacer
//! ce qui existe déjà à la destination (un `rename` ordinaire remplacerait
//! un dossier vide sans prévenir).
//!
//! Appel direct au système (via `libc`, déjà compilée pour Tauri) : la
//! bibliothèque standard ne propose pas cette variante.

// Seul appel au système de ce fichier, justifié ci-dessous.
#![allow(unsafe_code)]

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Renomme `from` en `to`, sauf si `to` existe déjà.
///
/// # Errors
///
/// `ErrorKind::AlreadyExists` si `to` existe ; sinon l'erreur du système.
pub fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: les deux chaînes sont terminées par un zéro et restent
    // valides pendant l'appel ; `renamex_np` ne les garde pas ensuite.
    let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
