//! Fichiers remplacés sur le disque : un éditeur
//! réécrit un export déjà au catalogue, ou une photo envoyée telle quelle.
//! PicLib retient la nouvelle date et la nouvelle taille du fichier, relit
//! ses dimensions et son orientation (un nouveau recadrage les change) et
//! refait sa vignette (et son aperçu) ; les métadonnées du catalogue (note,
//! mots-clés, titre...) restent.
//!
//! Lecture seule sur le disque : seuls le catalogue et les caches de
//! PicLib changent.

use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::params;

use crate::catalog::repositories::media;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaDetails, MediaId, Orientation};
use crate::files::{FileStamp, file_stamp};
use crate::metadata::read_metadata;

/// Écart de date de modification au-delà duquel un fichier est tenu pour
/// remplacé, en millisecondes (les systèmes de fichiers arrondissent ; même
/// règle que la synchronisation d'un dossier).
const MODIFIED_TOLERANCE_MS: i64 = 1000;

/// Prend acte des fichiers remplacés parmi les médias `ids` (ceux dont la
/// date ou la taille ne correspond plus au catalogue) : nouvelle date et
/// taille retenues, vignette à refaire. Renvoie ceux qui l'ont été ; un
/// fichier introuvable est laissé tel quel.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn refresh(catalog: &Mutex<Catalog>, ids: &[MediaId]) -> Result<Vec<MediaId>, CatalogError> {
    let known = media::read_with_paths(lock(catalog).connection(), ids)?;
    // Le disque est lu hors du verrou du catalogue.
    let replaced: Vec<Replaced> = known
        .into_iter()
        .filter_map(|(media, path)| {
            let stamp = file_stamp(&path).ok().flatten()?;
            let shifted =
                (stamp.modified.unix_millis() - media.data.file_modified_at.unix_millis()).abs()
                    >= MODIFIED_TOLERANCE_MS;
            if !shifted && stamp.size == media.data.file_size {
                return None;
            }
            let details = read_metadata(&path, media.data.format)
                .inspect_err(|error| {
                    tracing::warn!(%error, "cannot read the dimensions of a replaced file");
                })
                .ok()
                .map(|extracted| extracted.details);
            Some(Replaced {
                id: media.id,
                stamp,
                details,
            })
        })
        .collect();
    if replaced.is_empty() {
        return Ok(Vec::new());
    }
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    {
        let mut update = transaction.prepare_cached(
            "UPDATE media SET file_size = ?2, file_modified_at = ?3 WHERE id = ?1",
        )?;
        // Une orientation absente du nouveau fichier vaut « normale » :
        // l'éditeur a pu appliquer la rotation aux pixels.
        let mut resize = transaction.prepare_cached(
            "UPDATE media SET width = coalesce(?2, width), height = coalesce(?3, height),
                              orientation = ?4
             WHERE id = ?1",
        )?;
        let mut forget =
            transaction.prepare_cached("DELETE FROM thumbnails WHERE media_id = ?1")?;
        for file in &replaced {
            update.execute(params![
                file.id.get(),
                i64::try_from(file.stamp.size).unwrap_or(i64::MAX),
                file.stamp.modified.unix_millis()
            ])?;
            if let Some(details) = &file.details {
                resize.execute(params![
                    file.id.get(),
                    details.width,
                    details.height,
                    details.orientation.map(Orientation::get)
                ])?;
            }
            forget.execute([file.id.get()])?;
        }
    }
    transaction.commit()?;
    tracing::info!(files = replaced.len(), "replaced files refreshed");
    Ok(replaced.into_iter().map(|file| file.id).collect())
}

/// Fichier remplacé : son nouvel état, ses dimensions relues (`None` si le
/// fichier n'a pas pu être lu : elles restent).
struct Replaced {
    id: MediaId,
    stamp: FileStamp,
    details: Option<MediaDetails>,
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
