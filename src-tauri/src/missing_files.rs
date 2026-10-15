//! Vérification des fichiers manquants : un fichier
//! déplacé ou supprimé hors de PicLib, ou sur un disque débranché, est noté
//! manquant ; revenu, il ne l'est plus. Lecture seule sur le disque.
//!
//! Le catalogue est lu par morceaux, sans le tenir pendant qu'on interroge
//! le disque ; le résultat est écrit d'un coup, à la fin. Un dossier absent
//! (disque débranché) rend tous ses fichiers manquants sans les chercher un
//! par un.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::catalog::repositories::{media, missing};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::MediaId;
use crate::jobs::CancellationToken;

/// Médias lus à la fois.
const BATCH: usize = 2000;

/// Bilan d'une vérification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MissingReport {
    /// Fichiers cherchés.
    pub checked: u64,
    /// Fichiers introuvables.
    pub missing: u64,
    /// Fichiers manquants retrouvés.
    pub found_again: u64,
}

/// Erreur qui interrompt une vérification ; rien n'est alors écrit.
#[derive(Debug, thiserror::Error)]
pub enum MissingCheckError {
    /// Arrêtée à la demande de l'utilisateur.
    #[error("cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Cherche le fichier de chaque média et note ceux qui manquent.
/// `on_progress` reçoit le nombre de fichiers cherchés et le total.
///
/// # Errors
///
/// Voir [`MissingCheckError`].
pub fn check(
    catalog: &Mutex<Catalog>,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<MissingReport, MissingCheckError> {
    let total = media::count(lock(catalog).connection())?;
    let mut folders: HashMap<PathBuf, bool> = HashMap::new();
    let mut found_missing = HashSet::new();
    let mut checked = 0;
    let mut after = None;
    loop {
        if cancellation.is_cancelled() {
            return Err(MissingCheckError::Cancelled);
        }
        let batch = missing::locations_after(lock(catalog).connection(), after, BATCH)?;
        let Some(last) = batch.last() else {
            break;
        };
        after = Some(last.id);
        for location in &batch {
            let folder_exists = *folders
                .entry(location.folder.clone())
                .or_insert_with(|| location.folder.is_dir());
            // Un fichier qu'on ne peut pas interroger (droits) n'est pas
            // déclaré manquant.
            let exists = folder_exists
                && location
                    .folder
                    .join(&location.file_name)
                    .try_exists()
                    .unwrap_or(true);
            if !exists {
                found_missing.insert(location.id);
            }
        }
        checked += batch.len() as u64;
        on_progress(checked, total.max(checked));
    }
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    let previously = missing::missing_ids(&transaction)?;
    let newly: Vec<MediaId> = found_missing.difference(&previously).copied().collect();
    let again: Vec<MediaId> = previously.difference(&found_missing).copied().collect();
    missing::mark(&transaction, &newly, true)?;
    missing::mark(&transaction, &again, false)?;
    transaction.commit().map_err(CatalogError::from)?;
    Ok(MissingReport {
        checked,
        missing: found_missing.len() as u64,
        found_again: again.len() as u64,
    })
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
