//! Copies retrouvées après un arrêt brutal : vérifiées puis
//! nommées, elles n'avaient pas encore été inscrites au catalogue. Elles le
//! sont comme à l'import, métadonnées des fichiers comprises, et rattachées
//! à leur import.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use jiff::tz::TimeZone;

use super::copying::CopyDone;
use super::{ImportError, Placed, Session, lock};
use crate::catalog::Catalog;
use crate::catalog::repositories::file_operations::ItemId;
use crate::domain::{ImportCounts, ImportId};
use crate::files::{ScannedFile, list_folder};
use crate::import::folder_tree::FolderTree;
use crate::import::outcome::ImportOptions;
use crate::import::sidecars::Sidecars;
use crate::jobs::CancellationToken;

/// Copie nommée et vérifiée, retrouvée après un arrêt brutal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredCopy {
    /// Sa ligne restée « en attente » au journal.
    pub item: ItemId,
    /// Chemin de la copie.
    pub path: PathBuf,
    /// Empreinte vérifiée.
    pub content_hash: String,
}

/// Inscrit au catalogue les copies retrouvées, rattachées à l'import
/// `import_id` dont `destination` est le dossier de destination, et passe
/// leurs lignes du journal à « faites ». Les fichiers annexes (`.xmp`,
/// `.dop`) ne sont pas inscrits : leurs métadonnées sont lues avec la photo.
/// Le modèle de métadonnées et le chemin d'origine en légende ne sont pas
/// repris : seules les métadonnées des fichiers le sont.
///
/// # Errors
///
/// Catalogue illisible ; un dossier illisible laisse ses copies en attente.
pub fn register_recovered_copies(
    catalog: &Mutex<Catalog>,
    import_id: ImportId,
    destination: &Path,
    copies: &[RecoveredCopy],
    time_zone: &TimeZone,
) -> Result<ImportCounts, ImportError> {
    let options = ImportOptions {
        // Doublons déjà écartés avant la copie.
        skip_duplicates: false,
        ..ImportOptions::default()
    };
    let mut by_folder: BTreeMap<PathBuf, Vec<&RecoveredCopy>> = BTreeMap::new();
    for copy in copies {
        if let Some(folder) = copy.path.parent() {
            by_folder
                .entry(folder.to_path_buf())
                .or_default()
                .push(copy);
        }
    }
    let mut counts = ImportCounts::default();
    for (folder, copies) in by_folder {
        let report = match list_folder(&folder) {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(folder = %folder.display(), %error, "recovered copies left pending");
                continue;
            }
        };
        let files: Vec<&ScannedFile> = report
            .files
            .iter()
            .filter(|file| copies.iter().any(|copy| copy.path == file.path))
            .collect();
        let mut session = Session {
            import_id,
            source_root: destination,
            tree: FolderTree::new(destination),
            sidecars: Sidecars::new(&report.files, &report.sidecars),
            options: &options,
            time_zone,
            counts: ImportCounts::default(),
            unreadable: Vec::new(),
            copy: None,
        };
        let prepared = session.prepare(&files, &CancellationToken::default())?;
        let placed: Vec<Placed<'_, '_>> = prepared
            .iter()
            .map(|file| Placed {
                file,
                path: file.file.path.clone(),
                file_name: file.file_name.clone(),
                content_hash: copies
                    .iter()
                    .find(|copy| copy.path == file.file.path)
                    .map(|copy| copy.content_hash.clone()),
            })
            .collect();
        let done: Vec<CopyDone> = copies
            .iter()
            .map(|copy| CopyDone {
                item: copy.item,
                path: copy.path.clone(),
                source: None,
            })
            .collect();
        session.write(&mut lock(catalog), &placed, &done)?;
        counts.added_photos += session.counts.added_photos;
        counts.added_videos += session.counts.added_videos;
    }
    Ok(counts)
}
