//! Copie de sauvegarde des originaux pendant un import par copie : chaque fichier copié dans la destination (photo, vidéo,
//! fichier annexe) est aussi copié, intact et vérifié, dans un second
//! emplacement, sous `Importé le AAAA-MM-JJ HHMM`, avec son nom et
//! l'arborescence de la source. Ces copies n'entrent pas au catalogue ; elles
//! sont notées au journal (opération « sauvegarde »).
//!
//! Une sauvegarde impossible (disque plein, débranché) n'arrête pas l'import :
//! les fichiers non sauvegardés sont signalés dans le bilan.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{ImportError, Session, lock};
use crate::catalog::Catalog;
use crate::catalog::repositories::file_operations::{self, ItemResult, OperationId};
use crate::files::{CopyError, PlaceError, copy_verified, unique_path};
use crate::jobs::CancellationToken;

/// Nombre d'essais pour nommer une copie quand un autre programme prend le
/// nom choisi entre-temps.
const NAMING_ATTEMPTS: usize = 5;

/// Suivi de la copie de sauvegarde d'un import.
pub(super) struct BackupState {
    /// Dossier `Importé le …` de cet import.
    pub(super) folder: PathBuf,
    /// Opération du journal où chaque copie est notée.
    pub(super) operation: OperationId,
    /// Fichiers qui n'ont pas pu être sauvegardés.
    pub(super) failed: Vec<PathBuf>,
    /// Vrai quand le second emplacement ne peut plus rien recevoir.
    pub(super) stopped: bool,
}

/// Dossier de la sauvegarde d'un import lancé à `now` dans `root` :
/// `Importé le 2026-10-13 2230`, ou ` (2)`, ` (3)`... s'il existe déjà.
#[must_use]
pub(crate) fn backup_folder(root: &Path, now: &jiff::Zoned) -> PathBuf {
    let base = format!("Importé le {}", now.strftime("%Y-%m-%d %H%M"));
    let mut candidate = root.join(&base);
    let mut suffix = 2_u32;
    while candidate.exists() {
        candidate = root.join(format!("{base} ({suffix})"));
        suffix += 1;
    }
    candidate
}

impl Session<'_> {
    /// Sauvegarde les fichiers `sources`, copiés dans la destination.
    ///
    /// # Errors
    ///
    /// Annulation, ou catalogue illisible ; une sauvegarde impossible est
    /// seulement notée.
    pub(super) fn back_up(
        &mut self,
        catalog: &Mutex<Catalog>,
        sources: &[PathBuf],
        cancellation: &CancellationToken,
    ) -> Result<(), ImportError> {
        let source_root = self.source_root.to_path_buf();
        let Some(state) = self.copy.as_mut().and_then(|copy| copy.backup.as_mut()) else {
            return Ok(());
        };
        for source in sources {
            if state.stopped {
                state.failed.push(source.clone());
                continue;
            }
            let relative = source
                .parent()
                .and_then(|parent| parent.strip_prefix(&source_root).ok())
                .unwrap_or(Path::new(""));
            let folder = state.folder.join(relative);
            match back_up_file(catalog, state.operation, source, &folder, cancellation)? {
                Saved::Done => {}
                Saved::Failed => state.failed.push(source.clone()),
                Saved::Unavailable => {
                    state.failed.push(source.clone());
                    state.stopped = true;
                }
            }
        }
        Ok(())
    }
}

/// Issue de la sauvegarde d'un fichier.
enum Saved {
    Done,
    /// Ce fichier n'a pas pu être sauvegardé ; les autres continuent.
    Failed,
    /// Le second emplacement ne peut plus rien recevoir.
    Unavailable,
}

fn back_up_file(
    catalog: &Mutex<Catalog>,
    operation: OperationId,
    source: &Path,
    folder: &Path,
    cancellation: &CancellationToken,
) -> Result<Saved, ImportError> {
    let item = file_operations::record_pending(
        lock(catalog).connection(),
        operation,
        source,
        Some(folder),
    )?;
    let fail = |error: &CopyError| {
        tracing::warn!(path = %source.display(), %error, "file not backed up");
        file_operations::complete(
            lock(catalog).connection(),
            item,
            None,
            ItemResult::Failed,
            Some(&error.to_string()),
        )
    };
    let mut copy = match copy_verified(source, folder, cancellation) {
        Ok(copy) => copy,
        Err(CopyError::Cancelled) => return Err(ImportError::Cancelled),
        Err(error @ CopyError::Destination { .. }) => {
            fail(&error)?;
            return Ok(Saved::Unavailable);
        }
        Err(error) => {
            fail(&error)?;
            return Ok(Saved::Failed);
        }
    };
    for _ in 0..NAMING_ATTEMPTS {
        let target = own_name(folder, source);
        file_operations::set_verified(
            lock(catalog).connection(),
            item,
            &target,
            &copy.digest().to_hex(),
        )?;
        match copy.place(&target) {
            Ok(path) => {
                file_operations::complete(
                    lock(catalog).connection(),
                    item,
                    Some(&path),
                    ItemResult::Done,
                    None,
                )?;
                return Ok(Saved::Done);
            }
            Err(PlaceError::Taken(again)) => copy = again,
            Err(PlaceError::Failed(error)) => {
                fail(&error)?;
                return Ok(Saved::Unavailable);
            }
        }
    }
    fail(&CopyError::Destination {
        path: folder.to_path_buf(),
        source: std::io::Error::from(std::io::ErrorKind::AlreadyExists),
    })?;
    Ok(Saved::Failed)
}

/// Premier nom libre dans `folder` pour une copie qui garde le nom de
/// `source` (`-2`, `-3`... s'il est pris).
fn own_name(folder: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    match source.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => unique_path(folder, &stem, extension),
        None => folder.join(stem),
    }
}
