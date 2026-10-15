//! Import par copie, groupe par groupe : doublons écartés avant la copie,
//! copie vérifiée de chaque fichier et de ses fichiers annexes dans son
//! dossier de date, puis nom commun sans jamais écraser, et inscription au
//! catalogue aussitôt.
//!
//! Journal d'intention : chaque fichier est noté « en attente » avant sa
//! copie (avec son dossier), puis son nom définitif et son empreinte avant
//! d'être nommé ; le groupe est inscrit au catalogue dans la même
//! transaction que ses lignes « faites ». Après un arrêt brutal, les lignes
//! restées en attente disent quoi terminer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::super::copy::{ending_after, group_capture_time, group_key};
use super::{ImportError, Placed, PreparedFile, Session, lock};
use crate::catalog::Catalog;
use crate::catalog::repositories::file_operations::{self, ItemId, ItemResult, OperationId};
use crate::catalog::repositories::media;
use crate::domain::{CaptureTime, duplicate_name_key};
use crate::files::{CopyError, PendingCopy, PlaceError, copy_verified, unique_path, unique_stem};
use crate::jobs::CancellationToken;
use crate::renaming::{RenameInput, render};

/// Nombre d'essais pour nommer les copies d'un groupe quand un autre
/// programme prend les noms choisis entre-temps.
const NAMING_ATTEMPTS: usize = 5;

/// Copie d'un groupe en attente de son nom définitif.
struct GroupCopy<'p, 'f> {
    /// Photo ou vidéo copiée ; `None` pour un fichier annexe.
    file: Option<&'p PreparedFile<'f>>,
    /// Sa ligne « en attente » au journal.
    item: ItemId,
    source: PathBuf,
    /// Fin du nom après le nom commun du groupe (`.NEF`, `.NEF.xmp`) ;
    /// `None` : fichier annexe au nom inattendu, qui garde le sien.
    ending: Option<String>,
    copy: PendingCopy,
}

/// Copie nommée, à marquer « faite » au journal avec son inscription.
pub(super) struct CopyDone {
    pub(super) item: ItemId,
    pub(super) path: PathBuf,
    /// Fichier de la source copié (à sauvegarder) ; `None` pour une copie
    /// retrouvée après un arrêt brutal.
    pub(super) source: Option<PathBuf>,
}

impl<'f> Session<'_> {
    /// Copie les fichiers d'un lot, groupe par groupe, et inscrit chaque
    /// groupe au catalogue dès qu'il est copié : même si la suite s'arrête,
    /// aucune copie nommée ne reste hors du catalogue.
    ///
    /// # Errors
    ///
    /// Annulation, destination qui ne peut plus rien recevoir, catalogue
    /// illisible ; ce qui a été copié avant est inscrit.
    pub(super) fn copy_batch<'p>(
        &mut self,
        catalog: &Mutex<Catalog>,
        prepared: &'p [PreparedFile<'f>],
        cancellation: &CancellationToken,
        mut on_copied: impl FnMut(u64),
    ) -> Result<(), ImportError> {
        let mut groups: Vec<Vec<&'p PreparedFile<'f>>> = Vec::new();
        for file in prepared {
            let key = group_key(&file.file.path);
            match groups.last_mut() {
                Some(group) if key.is_some() && group_key(&group[0].file.path) == key => {
                    group.push(file);
                }
                _ => groups.push(vec![file]),
            }
        }

        for group in groups {
            if cancellation.is_cancelled() {
                return Err(ImportError::Cancelled);
            }
            let mut placed = Vec::new();
            let mut done = Vec::new();
            let copied = self.copy_group(catalog, &group, cancellation, &mut placed, &mut done);
            // Les copies déjà nommées sont inscrites même si la suite du
            // groupe a échoué.
            self.write(&mut lock(catalog), &placed, &done)?;
            copied?;
            let sources: Vec<PathBuf> =
                done.iter().filter_map(|copy| copy.source.clone()).collect();
            self.back_up(catalog, &sources, cancellation)?;
            on_copied(group.len() as u64);
        }
        Ok(())
    }

    /// Copie un groupe (une photo, ou une paire RAW + JPEG) et ses fichiers
    /// annexes dans son dossier de date, sous un nom commun ; les photos et
    /// vidéos copiées sont ajoutées à `placed`, toutes les copies nommées à
    /// `done`.
    fn copy_group<'p>(
        &mut self,
        catalog: &Mutex<Catalog>,
        group: &[&'p PreparedFile<'f>],
        cancellation: &CancellationToken,
        placed: &mut Vec<Placed<'p, 'f>>,
        done: &mut Vec<CopyDone>,
    ) -> Result<(), ImportError> {
        let Some(state) = self.copy.as_ref() else {
            return Ok(());
        };
        let destination_root = state.options.destination.clone();
        let arrangement = state.options.arrangement.clone();
        let operation = state.operation;

        let members = self.without_duplicates(catalog, group)?;
        let capture_times: Vec<CaptureTime> =
            members.iter().map(|file| self.capture_time(file)).collect();
        let Some(capture_time) = group_capture_time(&capture_times) else {
            return Ok(());
        };
        let folder = destination_root.join(arrangement.relative_folder(
            self.source_root,
            &group[0].file.path,
            &capture_time,
        ));

        let mut copies: Vec<GroupCopy<'p, 'f>> = Vec::new();
        for file in members {
            let Some((item, copy)) =
                Self::copy_file(catalog, operation, &file.file.path, &folder, cancellation)?
            else {
                self.counts.copy_failures += 1;
                if let Some(state) = self.copy.as_mut() {
                    state.failed.push(file.file.path.clone());
                }
                continue;
            };
            let digest = copy.digest().to_hex();
            let is_known = self
                .copy
                .as_ref()
                .is_some_and(|state| state.digests.contains(&digest))
                || media::find_by_content_hash(lock(catalog).connection(), &digest)?.is_some();
            if self.options.skip_duplicates && is_known {
                // Même contenu déjà au catalogue sous un autre nom : la copie
                // provisoire est abandonnée (effacée), rien n'est à noter.
                file_operations::forget(lock(catalog).connection(), item)?;
                self.counts.duplicates_skipped += 1;
                continue;
            }
            if let Some(state) = self.copy.as_mut() {
                state.digests.insert(digest);
            }
            let stem = stem_of(&file.file.path);
            copies.push(GroupCopy {
                file: Some(file),
                item,
                source: file.file.path.clone(),
                ending: ending_after(&file.file_name, &stem),
                copy,
            });
        }
        let Some(first) = copies.first() else {
            return Ok(());
        };
        let stem = stem_of(&first.source);

        // Fichiers annexes des photos copiées.
        let sidecars: BTreeSet<PathBuf> = copies
            .iter()
            .filter_map(|copy| copy.file)
            .flat_map(|file| {
                let xmp = self.sidecars.find_xmp(file.file).map(|found| found.path);
                let dop = self.sidecars.find_dop(file.file);
                xmp.into_iter().chain(dop).map(Path::to_path_buf)
            })
            .collect();
        for sidecar in sidecars {
            let Some((item, copy)) =
                Self::copy_file(catalog, operation, &sidecar, &folder, cancellation)?
            else {
                if let Some(state) = self.copy.as_mut() {
                    state.sidecars_not_copied.push(sidecar);
                }
                continue;
            };
            let name = sidecar
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            copies.push(GroupCopy {
                file: None,
                item,
                ending: ending_after(name, &stem),
                source: sidecar,
                copy,
            });
        }

        let target_stem = self.target_stem(&copies, &stem, &capture_time);
        Self::name_copies(catalog, &folder, &target_stem, copies, placed, done)
    }

    /// Nom commun des copies d'un groupe : celui de la source, ou celui que
    /// donne le modèle de renommage, numéroté groupe par groupe.
    fn target_stem(
        &mut self,
        copies: &[GroupCopy<'_, 'f>],
        stem: &str,
        capture_time: &CaptureTime,
    ) -> String {
        let Some(state) = self.copy.as_mut() else {
            return stem.to_owned();
        };
        let (Some(template), Some(file)) = (
            state.options.rename.as_ref(),
            copies.iter().find_map(|copy| copy.file),
        ) else {
            return stem.to_owned();
        };
        let details = &file.metadata.details;
        let name = render(
            template,
            RenameInput {
                capture_time,
                original_name: &file.file_name,
                current_name: &file.file_name,
                camera: details
                    .camera_model
                    .as_deref()
                    .or(details.camera_make.as_deref()),
            },
            state.sequence,
        );
        state.sequence = state.sequence.saturating_add(1);
        if name.is_empty() {
            stem.to_owned()
        } else {
            name
        }
    }

    /// Fichiers d'un groupe qui ne sont pas des doublons d'une photo du
    /// catalogue : ils sont écartés avant de rien copier.
    fn without_duplicates<'p>(
        &mut self,
        catalog: &Mutex<Catalog>,
        group: &[&'p PreparedFile<'f>],
    ) -> Result<Vec<&'p PreparedFile<'f>>, ImportError> {
        let mut members = Vec::with_capacity(group.len());
        for &file in group {
            if self.options.skip_duplicates
                && media::find_duplicate(
                    lock(catalog).connection(),
                    &duplicate_name_key(&file.file_name),
                    file.file.size,
                    &self.capture_time(file),
                )?
                .is_some()
            {
                self.counts.duplicates_skipped += 1;
                continue;
            }
            members.push(file);
        }
        Ok(members)
    }

    /// Copie un fichier dans `folder`, sous un nom provisoire, après l'avoir
    /// noté « en attente » au journal. `None` : la source n'a pas pu être
    /// copiée (noté au journal), les autres fichiers continuent.
    ///
    /// # Errors
    ///
    /// Annulation, ou destination qui ne peut plus rien recevoir (la ligne
    /// en attente est close à la fin de l'import).
    fn copy_file(
        catalog: &Mutex<Catalog>,
        operation: OperationId,
        source: &Path,
        folder: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Option<(ItemId, PendingCopy)>, ImportError> {
        let item = file_operations::record_pending(
            lock(catalog).connection(),
            operation,
            source,
            Some(folder),
        )?;
        match copy_verified(source, folder, cancellation) {
            Ok(copy) => Ok(Some((item, copy))),
            Err(CopyError::Cancelled) => Err(ImportError::Cancelled),
            Err(error @ CopyError::Destination { .. }) => Err(ImportError::Destination(error)),
            Err(error) => {
                tracing::warn!(path = %source.display(), %error, "file not copied");
                file_operations::complete(
                    lock(catalog).connection(),
                    item,
                    None,
                    ItemResult::Failed,
                    Some(&error.to_string()),
                )?;
                Ok(None)
            }
        }
    }

    /// Donne aux copies d'un groupe leur nom définitif : le premier nom
    /// commun libre pour tout le groupe, sans jamais écraser. Avant de
    /// nommer, le nom choisi et l'empreinte de chaque copie sont notés au
    /// journal ; les copies nommées sont ajoutées à `done`, les photos et
    /// vidéos aussi à `placed`.
    fn name_copies<'p>(
        catalog: &Mutex<Catalog>,
        folder: &Path,
        stem: &str,
        mut copies: Vec<GroupCopy<'p, 'f>>,
        placed: &mut Vec<Placed<'p, 'f>>,
        done: &mut Vec<CopyDone>,
    ) -> Result<(), ImportError> {
        for _ in 0..NAMING_ATTEMPTS {
            let endings: Vec<String> = copies
                .iter()
                .filter_map(|copy| copy.ending.clone())
                .collect();
            let shared = unique_stem(folder, stem, &endings);
            let targets: Vec<PathBuf> = copies
                .iter()
                .map(|copy| match &copy.ending {
                    Some(ending) => folder.join(format!("{shared}{ending}")),
                    None => own_name(folder, &copy.source),
                })
                .collect();
            Self::note_targets(catalog, &copies, &targets)?;
            let mut taken = Vec::new();
            for (
                GroupCopy {
                    file,
                    item,
                    source,
                    ending,
                    copy,
                },
                target,
            ) in copies.into_iter().zip(targets)
            {
                let digest = copy.digest().to_hex();
                match copy.place(&target) {
                    Ok(path) => {
                        done.push(CopyDone {
                            item,
                            path: path.clone(),
                            source: Some(source.clone()),
                        });
                        if let Some(file) = file {
                            placed.push(Placed {
                                file,
                                file_name: path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or(&file.file_name)
                                    .to_owned(),
                                path,
                                content_hash: Some(digest),
                            });
                        }
                    }
                    Err(PlaceError::Taken(copy)) => taken.push(GroupCopy {
                        file,
                        item,
                        source,
                        ending,
                        copy,
                    }),
                    Err(PlaceError::Failed(error)) => return Err(ImportError::Destination(error)),
                }
            }
            if taken.is_empty() {
                return Ok(());
            }
            copies = taken;
        }
        Err(ImportError::Destination(CopyError::Destination {
            path: folder.to_path_buf(),
            source: std::io::Error::from(std::io::ErrorKind::AlreadyExists),
        }))
    }
}

impl Session<'_> {
    /// Note au journal, en une transaction, le nom que chaque copie va
    /// prendre et son empreinte : après un arrêt brutal, une copie nommée
    /// se retrouve et se vérifie.
    fn note_targets(
        catalog: &Mutex<Catalog>,
        copies: &[GroupCopy<'_, '_>],
        targets: &[PathBuf],
    ) -> Result<(), ImportError> {
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        for (copy, target) in copies.iter().zip(targets) {
            file_operations::set_verified(
                &transaction,
                copy.item,
                target,
                &copy.copy.digest().to_hex(),
            )?;
        }
        transaction
            .commit()
            .map_err(crate::catalog::CatalogError::from)?;
        Ok(())
    }
}

/// Nom du fichier sans son extension.
fn stem_of(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// Premier nom libre dans `folder` pour un fichier annexe qui garde son
/// propre nom.
fn own_name(folder: &Path, source: &Path) -> PathBuf {
    let stem = stem_of(source);
    match source.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => unique_path(folder, &stem, extension),
        None => folder.join(stem),
    }
}
