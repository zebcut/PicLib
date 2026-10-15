//! « Lire les métadonnées des fichiers » : relit les fichiers de
//! médias déjà au catalogue — fichiers annexes XMP et `.dop`, XMP et IPTC
//! intégrés, EXIF — dans le même ordre de priorité qu'à l'import.
//!
//! **Lecture seule** : ni les fichiers ni le catalogue ne sont modifiés ici.
//! Ce qui est trouvé sert ensuite à compléter ce qui manque, en une seule
//! modification annulable ([`crate::edits::Edit::file_metadata`]).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::catalog::repositories::media;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{CaptureTime, MediaDetails, MediaId};
use crate::files::{ScannedFile, list_folder};
use crate::import::{Sidecars, recover};
use crate::jobs::CancellationToken;
use crate::metadata::{ExtractedMetadata, MetadataError, RecoveredMetadata, read_metadata};

/// Ce que les fichiers d'un média disent de lui.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundMetadata {
    /// Média concerné.
    pub media_id: MediaId,
    /// Date de prise de vue : celle de l'EXIF, sinon celle du XMP, du
    /// `.dop` ou de l'IPTC (comme à l'import).
    pub capture_time: Option<CaptureTime>,
    /// Informations techniques ; la position GPS est celle retenue parmi
    /// toutes les sources.
    pub details: MediaDetails,
    /// Note, marqueur, couleur, champs descriptifs et mots-clés retenus
    /// parmi les sources.
    pub metadata: RecoveredMetadata,
}

/// Bilan d'une lecture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadReport {
    /// Ce qui a été trouvé, pour chaque média dont le fichier a pu être lu.
    pub found: Vec<FoundMetadata>,
    /// Médias dont le fichier est introuvable ou illisible (disque
    /// débranché, fichier déplacé…).
    pub missing: u64,
    /// Médias dont une source de métadonnées n'a pas pu être lue (les
    /// autres ont servi).
    pub unreadable_metadata: u64,
}

/// Lecture interrompue.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// L'utilisateur a annulé : rien n'est retenu.
    #[error("reading cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Fichier d'un média à relire.
struct Target {
    media_id: MediaId,
    path: PathBuf,
}

/// Relit les fichiers des médias `ids` (les identifiants inconnus sont
/// ignorés). Le catalogue n'est verrouillé que le temps de trouver les
/// chemins : la lecture des fichiers, la partie lente, se fait sans lui.
/// `on_progress` reçoit le nombre de médias traités et le total.
///
/// # Errors
///
/// Voir [`ReadError`].
pub fn read_files(
    catalog: &Mutex<Catalog>,
    ids: &[MediaId],
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ReadReport, ReadError> {
    let located = media::read_with_paths(lock(catalog).connection(), ids)?;
    // Par dossier : chacun n'est parcouru qu'une fois, pour ses fichiers
    // annexes.
    let mut by_folder: BTreeMap<PathBuf, Vec<Target>> = BTreeMap::new();
    for (media, path) in located {
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        by_folder.entry(folder).or_default().push(Target {
            media_id: media.id,
            path,
        });
    }

    let total = by_folder.values().map(Vec::len).sum::<usize>() as u64;
    let mut done = 0_u64;
    let mut report = ReadReport::default();
    on_progress(0, total);
    for (folder, targets) in by_folder {
        let Some(listing) = FolderListing::list(&folder) else {
            report.missing += targets.len() as u64;
            done += targets.len() as u64;
            on_progress(done, total);
            continue;
        };
        for target in targets {
            if cancellation.is_cancelled() {
                return Err(ReadError::Cancelled);
            }
            match listing.metadata_of(&target) {
                Some((found, has_unreadable_metadata)) => {
                    report.unreadable_metadata += u64::from(has_unreadable_metadata);
                    report.found.push(found);
                }
                None => report.missing += 1,
            }
            done += 1;
            on_progress(done, total);
        }
    }
    Ok(report)
}

/// Contenu d'un dossier : ses photos et vidéos, et leurs fichiers annexes.
struct FolderListing {
    files: HashMap<PathBuf, ScannedFile>,
    sidecars: Sidecars,
}

impl FolderListing {
    /// `None` si le dossier est introuvable ou illisible.
    fn list(folder: &Path) -> Option<Self> {
        match list_folder(folder) {
            Ok(scan) => Some(Self {
                sidecars: Sidecars::new(&scan.files, &scan.sidecars),
                files: scan
                    .files
                    .into_iter()
                    .map(|file| (file.path.clone(), file))
                    .collect(),
            }),
            Err(error) => {
                tracing::warn!(%error, "folder cannot be listed, its media are skipped");
                None
            }
        }
    }

    /// Métadonnées du fichier d'un média, et vrai si une source était
    /// illisible ; `None` si le fichier est introuvable ou ne peut pas être
    /// ouvert.
    fn metadata_of(&self, target: &Target) -> Option<(FoundMetadata, bool)> {
        let Some(file) = self.files.get(&target.path) else {
            tracing::warn!(path = %target.path.display(), "file not found, skipped");
            return None;
        };
        let (extracted, unreadable) = match read_metadata(&file.path, file.format) {
            Ok(extracted) => (extracted, false),
            Err(MetadataError::Unreadable(reason)) => {
                tracing::warn!(path = %file.path.display(), %reason, "unreadable metadata");
                (ExtractedMetadata::default(), true)
            }
            Err(MetadataError::Io(error)) => {
                tracing::warn!(path = %file.path.display(), %error, "file cannot be read, skipped");
                return None;
            }
        };
        let recovery = recover(file, &extracted, &self.sidecars);
        let found = FoundMetadata {
            media_id: target.media_id,
            capture_time: extracted.capture_time.or(recovery.metadata.capture_time),
            details: MediaDetails {
                gps: recovery.metadata.gps,
                ..extracted.details
            },
            metadata: recovery.metadata,
        };
        Some((found, unreadable || recovery.has_unreadable_metadata))
    }
}

/// Voir `import::reference::lock`.
fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
