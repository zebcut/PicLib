//! Remplissage automatique du lieu des médias géolocalisés.

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::{PlaceDataError, PlaceIndex};
use crate::catalog::repositories::places;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::Language;
use crate::jobs::CancellationToken;
use crate::keywords::places as place_keywords;

/// Nombre de médias traités entre deux écritures au catalogue : le catalogue
/// n'est jamais bloqué plus de quelques millisecondes.
const BATCH_SIZE: usize = 500;

/// Bilan d'un remplissage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FillReport {
    /// Médias géolocalisés traités.
    pub looked_up: u64,
    /// Médias dont au moins un champ a été rempli.
    pub filled: u64,
}

/// Erreur qui interrompt le remplissage.
#[derive(Debug, thiserror::Error)]
pub enum FillError {
    /// L'utilisateur a annulé ; les médias déjà traités le restent.
    #[error("place lookup cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// La base des lieux intégrée est mal formée.
    #[error(transparent)]
    Data(#[from] PlaceDataError),
}

/// Cherche le lieu de chaque média géolocalisé jamais traité et remplit ses
/// champs Pays, Région et Ville vides ; le nom du pays est écrit dans
/// `language`. `on_progress` reçoit le nombre de médias traités et le total.
///
/// Ce remplissage n'entre pas dans l'historique d'annulation (Cmd+Z) : ce
/// n'est pas un geste de l'utilisateur.
///
/// # Errors
///
/// Voir [`FillError`].
pub fn fill_pending(
    catalog: &Mutex<Catalog>,
    language: Language,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<FillReport, FillError> {
    // Rattrapage des médias déjà au catalogue, une seule fois :
    // cette tâche tourne au démarrage, en tâche de fond.
    {
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        place_keywords::backfill(&transaction)?;
        transaction.commit().map_err(CatalogError::from)?;
    }
    let pending = places::pending(lock(catalog).connection())?;
    let mut report = FillReport::default();
    if pending.is_empty() {
        return Ok(report);
    }
    // La base n'est lue que s'il y a du travail : rien au démarrage d'un
    // catalogue déjà traité.
    let index = PlaceIndex::load()?;
    let total = pending.len() as u64;
    on_progress(0, total);

    for batch in pending.chunks(BATCH_SIZE) {
        if cancellation.is_cancelled() {
            return Err(FillError::Cancelled);
        }
        // Les recherches se font sans bloquer le catalogue.
        let found: Vec<_> = batch
            .iter()
            .map(|position| {
                let names = index.lookup(position.latitude, position.longitude, language);
                (position.media_id, names)
            })
            .collect();
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        let ids: Vec<_> = found.iter().map(|(media_id, _)| *media_id).collect();
        let before = place_keywords::read(&transaction, &ids)?;
        for (media_id, names) in &found {
            if places::fill(&transaction, *media_id, names.as_ref())? {
                report.filled += 1;
            }
        }
        // Les champs remplis deviennent des mots-clés de lieu.
        let after = place_keywords::read(&transaction, &ids)?;
        place_keywords::changes(&transaction, &before, &after)?.apply(&transaction)?;
        transaction.commit().map_err(CatalogError::from)?;
        drop(catalog);
        report.looked_up += batch.len() as u64;
        on_progress(report.looked_up, total);
    }
    Ok(report)
}

/// Verrouille le catalogue (un verrou « empoisonné » protège quand même une
/// connexion utilisable : chaque écriture est dans une transaction).
fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
