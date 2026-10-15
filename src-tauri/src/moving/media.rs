//! Déplacer ou renommer des photos : chaque photo part avec ses fichiers annexes (`.xmp`, `.dop`) et
//! l'autre fichier de sa paire RAW + JPEG, sous un même nom de base, sans
//! jamais rien écraser (suffixe `-2`, `-3`…). Sur un même disque seulement :
//! pour le système, ce n'est qu'un renommage, instantané.
//!
//! Comme toute opération sur les fichiers, chaque fichier
//! est noté « en attente » au journal, déplacé, puis noté fait ; le
//! catalogue suit dans une transaction. Cmd+Z remet les fichiers à leur
//! place (et leur nom).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::FolderMoveError;
use crate::catalog::repositories::file_operations::{
    self, ItemResult, OperationKind, OperationStatus,
};
use crate::catalog::repositories::{folders, media};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaId, Timestamp, duplicate_name_key};
use crate::edits::{MediaPlace, PlacementChange, PlacementEdit};
use crate::files::move_file;
use crate::media_files;

/// Déplacement ou renommage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaMoveKind {
    /// Photos déplacées dans un autre dossier.
    Moved,
    /// Photos renommées.
    Renamed,
}

/// Fichiers qui partagent un nom de base : une photo (ou une paire RAW +
/// JPEG) et ses fichiers annexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGroup {
    /// Médias du groupe : emplacement avant et après.
    pub media: Vec<(MediaId, MediaPlace, MediaPlace)>,
    /// Tous ses fichiers, médias d'abord : de, vers.
    pub files: Vec<(PathBuf, PathBuf)>,
}

/// Ce qu'un déplacement ou un renommage ferait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePlan {
    /// Déplacement ou renommage.
    pub kind: MediaMoveKind,
    /// Groupes à déplacer.
    pub groups: Vec<FileGroup>,
}

impl MovePlan {
    /// Médias déplacés (ou renommés).
    #[must_use]
    pub fn media_count(&self) -> usize {
        self.groups.iter().map(|group| group.media.len()).sum()
    }

    /// Fichiers annexes qui les suivent.
    #[must_use]
    pub fn sidecar_count(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.files.len() - group.media.len())
            .sum()
    }
}

/// Photos d'un même dossier qui partagent un nom de base, avec leurs
/// fichiers annexes, telles qu'elles sont au catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceGroup {
    /// Dossier.
    pub folder: PathBuf,
    /// Nom de base (celui de la première photo, casse comprise).
    pub stem: String,
    /// Médias et leur nom de fichier.
    pub media: Vec<(MediaId, String)>,
    /// Fichiers annexes.
    pub sidecars: Vec<PathBuf>,
}

impl SourceGroup {
    /// Fichiers du groupe et leur fin de nom après le nom de base
    /// (`.NEF`, `.NEF.xmp`…) ; `None` pour un nom qui ne commence pas par
    /// le nom de base (il garde alors son nom).
    pub(crate) fn endings(&self) -> Vec<(PathBuf, Option<String>)> {
        let media = self.media.iter().map(|(_, name)| self.folder.join(name));
        media
            .chain(self.sidecars.iter().cloned())
            .map(|path| {
                let name = path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                let ending = ending_after(&name, &self.stem);
                (path, ending)
            })
            .collect()
    }
}

/// Fin de `name` après le nom de base `stem` (sans tenir compte des
/// majuscules) ; `None` si `name` ne commence pas par `stem`.
fn ending_after(name: &str, stem: &str) -> Option<String> {
    let cut = stem.len();
    (name.is_char_boundary(cut) && duplicate_name_key(&name[..cut]) == duplicate_name_key(stem))
        .then(|| name[cut..].to_owned())
}

/// Nom de base d'un nom de fichier.
fn stem_of(name: &str) -> String {
    Path::new(name).file_stem().map_or_else(
        || name.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// Les médias `ids`, avec l'autre fichier de leur paire (même nom de base,
/// même dossier) et leurs fichiers annexes, regroupés par nom de base.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub(crate) fn source_groups(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<SourceGroup>, CatalogError> {
    let selected = media::read_with_paths(connection, ids)?;
    let mut expanded: Vec<MediaId> = Vec::new();
    let mut seen = HashSet::new();
    let mut folders_done = HashMap::new();
    for (found, _) in &selected {
        let folder = found.data.folder_id;
        if let std::collections::hash_map::Entry::Vacant(entry) = folders_done.entry(folder) {
            entry.insert(media::list_in_folder(connection, folder)?);
        }
        let stem = duplicate_name_key(&stem_of(&found.data.file_name));
        for sibling in &folders_done[&folder] {
            if duplicate_name_key(&stem_of(&sibling.data.file_name)) == stem
                && seen.insert(sibling.id)
            {
                expanded.push(sibling.id);
            }
        }
    }
    let files = media_files::files_of(connection, &expanded)?;
    let mut groups: BTreeMap<(PathBuf, String), SourceGroup> = BTreeMap::new();
    for file in files {
        let (Some(folder), Some(name)) = (
            file.path.parent().map(Path::to_path_buf),
            file.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        ) else {
            continue;
        };
        let stem = stem_of(&name);
        let group = groups
            .entry((folder.clone(), duplicate_name_key(&stem)))
            .or_insert_with(|| SourceGroup {
                folder,
                stem,
                media: Vec::new(),
                sidecars: Vec::new(),
            });
        group.media.push((file.media_id, name));
        group.sidecars.extend(file.sidecars);
    }
    Ok(groups.into_values().collect())
}

/// Premier nom de base libre dans `dir` parmi `base`, `base-2`… pour tous
/// les fichiers du groupe : ni sur le disque, ni pris par un média du
/// catalogue de ce dossier, ni par un autre groupe (`taken`), sauf par les
/// fichiers du groupe lui-même (`own`, un renommage qui ne change que la
/// casse).
pub(crate) fn free_stem(
    connection: &Connection,
    dir: &Path,
    base: &str,
    endings: &[String],
    own: &HashSet<PathBuf>,
    taken: &mut HashSet<String>,
) -> Result<String, CatalogError> {
    let known = folders::find_by_path(connection, dir)?;
    let is_free = |candidate: &str| -> Result<bool, CatalogError> {
        for ending in endings {
            let name = format!("{candidate}{ending}");
            let path = dir.join(&name);
            if taken.contains(&duplicate_name_key(&path.to_string_lossy())) {
                return Ok(false);
            }
            let on_disk = path.symlink_metadata().is_ok() && !own.contains(&path);
            let in_catalog = match &known {
                Some(folder) => media::find_in_folder(connection, folder.id, &name)?
                    .is_some_and(|_| !own.contains(&path)),
                None => false,
            };
            if on_disk || in_catalog {
                return Ok(false);
            }
        }
        Ok(true)
    };
    let mut candidate = base.to_owned();
    let mut suffix = 2_u32;
    while !is_free(&candidate)? {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    for ending in endings {
        taken.insert(duplicate_name_key(
            &dir.join(format!("{candidate}{ending}")).to_string_lossy(),
        ));
    }
    Ok(candidate)
}

/// Le groupe `source`, sous le nom de base `stem` dans `dir`.
pub(crate) fn planned_group(source: &SourceGroup, dir: &Path, stem: &str) -> FileGroup {
    let endings = source.endings();
    let files: Vec<(PathBuf, PathBuf)> = endings
        .iter()
        .map(|(from, ending)| {
            let name = ending.as_ref().map_or_else(
                || {
                    from.file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
                },
                |ending| format!("{stem}{ending}"),
            );
            (from.clone(), dir.join(name))
        })
        .collect();
    let media = source
        .media
        .iter()
        .zip(&files)
        .map(|((id, name), (_, to))| {
            (
                *id,
                MediaPlace {
                    folder: source.folder.clone(),
                    file_name: name.clone(),
                },
                MediaPlace {
                    folder: dir.to_path_buf(),
                    file_name: to
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                },
            )
        })
        .collect();
    FileGroup { media, files }
}

/// Fins de nom d'un groupe (les fichiers qui gardent leur nom en sont
/// exclus).
pub(crate) fn group_endings(source: &SourceGroup) -> Vec<String> {
    source
        .endings()
        .into_iter()
        .filter_map(|(_, ending)| ending)
        .collect()
}

/// Photos déplacées ou renommées : de quoi l'annuler (Cmd+Z) et le rétablir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaMove {
    kind: MediaMoveKind,
    files: Vec<(PathBuf, PathBuf)>,
    placement: PlacementEdit,
    media_count: usize,
}

impl MediaMove {
    /// Déplacement ou renommage.
    #[must_use]
    pub fn kind(&self) -> MediaMoveKind {
        self.kind
    }

    /// Nombre de médias concernés.
    #[must_use]
    pub fn media_count(&self) -> usize {
        self.media_count
    }

    /// Remet les fichiers à leur place (Cmd+Z).
    ///
    /// # Errors
    ///
    /// Voir [`FolderMoveError`] ; rien n'est alors changé.
    pub fn undo(&self, catalog: &mut Catalog) -> Result<(), FolderMoveError> {
        let back: Vec<(PathBuf, PathBuf)> = self
            .files
            .iter()
            .rev()
            .map(|(from, to)| (to.clone(), from.clone()))
            .collect();
        move_all(catalog, &back)?;
        let transaction = catalog.transaction()?;
        self.placement.revert(&transaction)?;
        transaction.commit().map_err(CatalogError::from)?;
        Ok(())
    }

    /// Déplace de nouveau les fichiers (Cmd+Maj+Z).
    ///
    /// # Errors
    ///
    /// Voir [`FolderMoveError`] ; rien n'est alors changé.
    pub fn redo(&self, catalog: &mut Catalog) -> Result<(), FolderMoveError> {
        move_all(catalog, &self.files)?;
        let transaction = catalog.transaction()?;
        self.placement.apply(&transaction)?;
        transaction.commit().map_err(CatalogError::from)?;
        Ok(())
    }
}

/// Résultat de l'exécution d'un plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveOutcome {
    /// Ce qui a été fait ; `None` si rien n'a pu l'être.
    pub done: Option<MediaMove>,
    /// Groupes laissés en place (le système a refusé).
    pub failed: usize,
}

/// Exécute le plan : groupe par groupe, chaque fichier au journal ; un
/// groupe dont une photo ne peut pas bouger est remis en place en entier.
/// Le catalogue suit les groupes déplacés.
///
/// # Errors
///
/// Échoue si le catalogue ne peut pas être écrit (les fichiers déplacés le
/// restent : la reprise au démarrage termine, grâce au journal).
pub fn execute(catalog: &mut Catalog, plan: MovePlan) -> Result<MoveOutcome, CatalogError> {
    let operation = file_operations::start(
        catalog.connection(),
        OperationKind::Move,
        None,
        Timestamp::now(),
    )?;
    let mut done_groups = Vec::new();
    let mut failed = 0;
    for group in plan.groups {
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut group_failed = false;
        for (position, (from, to)) in group.files.iter().enumerate() {
            let is_media = position < group.media.len();
            if move_one(catalog, operation, from, to)? {
                moved.push((from.clone(), to.clone()));
            } else if is_media {
                group_failed = true;
                break;
            }
        }
        if group_failed {
            failed += 1;
            for (from, to) in moved.iter().rev() {
                if let Err(error) = move_file(to, from) {
                    tracing::error!(%error, file = %to.display(), "cannot put a file back");
                }
            }
            continue;
        }
        done_groups.push((group.media, moved));
    }
    let status = if failed == 0 {
        OperationStatus::Completed
    } else {
        OperationStatus::Failed
    };
    if done_groups.is_empty() {
        file_operations::finish(catalog.connection(), operation, status, Timestamp::now())?;
        return Ok(MoveOutcome { done: None, failed });
    }
    let transaction = catalog.transaction()?;
    let changes = done_groups
        .iter()
        .flat_map(|(media, _)| media.iter())
        .map(|(id, before, after)| PlacementChange {
            media: *id,
            before: before.clone(),
            after: after.clone(),
            missing_before: false,
            missing_after: false,
            stack: None,
        })
        .collect();
    let placement = PlacementEdit::new(&transaction, changes)?;
    placement.apply(&transaction)?;
    file_operations::finish(&transaction, operation, status, Timestamp::now())?;
    transaction.commit()?;
    let media_count = done_groups.iter().map(|(media, _)| media.len()).sum();
    Ok(MoveOutcome {
        done: Some(MediaMove {
            kind: plan.kind,
            files: done_groups
                .into_iter()
                .flat_map(|(_, files)| files)
                .collect(),
            placement,
            media_count,
        }),
        failed,
    })
}

/// Déplace un fichier, noté au journal ; renvoie faux si le système a
/// refusé.
fn move_one(
    catalog: &Catalog,
    operation: file_operations::OperationId,
    from: &Path,
    to: &Path,
) -> Result<bool, CatalogError> {
    let connection = catalog.connection();
    let item = file_operations::record_pending(connection, operation, from, Some(to))?;
    match move_file(from, to) {
        Ok(()) => {
            file_operations::complete(connection, item, None, ItemResult::Done, None)?;
            Ok(true)
        }
        Err(error) => {
            let reason = error.to_string();
            tracing::warn!(%error, file = %from.display(), "cannot move a file");
            file_operations::complete(connection, item, None, ItemResult::Failed, Some(&reason))?;
            Ok(false)
        }
    }
}

/// Déplace tous les fichiers, ou aucun : au premier refus, ceux déjà
/// déplacés reviennent.
fn move_all(catalog: &Catalog, files: &[(PathBuf, PathBuf)]) -> Result<(), FolderMoveError> {
    let operation = file_operations::start(
        catalog.connection(),
        OperationKind::Move,
        None,
        Timestamp::now(),
    )?;
    let mut moved: Vec<&(PathBuf, PathBuf)> = Vec::new();
    for pair in files {
        if move_one(catalog, operation, &pair.0, &pair.1)? {
            moved.push(pair);
            continue;
        }
        for (from, to) in moved.iter().rev() {
            if let Err(error) = move_file(to, from) {
                tracing::error!(%error, file = %to.display(), "cannot put a file back");
            }
        }
        file_operations::finish(
            catalog.connection(),
            operation,
            OperationStatus::Failed,
            Timestamp::now(),
        )?;
        return Err(FolderMoveError::Disk(format!(
            "{} cannot be moved",
            pair.0.display()
        )));
    }
    file_operations::finish(
        catalog.connection(),
        operation,
        OperationStatus::Completed,
        Timestamp::now(),
    )?;
    Ok(())
}

/// Raison pour laquelle des photos ne peuvent pas être déplacées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaMoveRefusal {
    /// Elles sont déjà toutes dans ce dossier.
    NothingToMove,
    /// Le fichier d'une photo est introuvable.
    Missing,
    /// Le dossier de destination est introuvable.
    DestinationMissing,
    /// La destination est sur un autre disque.
    OtherDisk,
    /// Une photo ou la destination est sur une carte mémoire.
    CameraCard,
}

/// Prépare le déplacement des photos `ids` (avec l'autre fichier de leur
/// paire et leurs fichiers annexes) dans `destination`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn plan_move(
    connection: &Connection,
    ids: &[MediaId],
    destination: &Path,
) -> Result<Result<MovePlan, MediaMoveRefusal>, CatalogError> {
    if !destination.is_dir() {
        return Ok(Err(MediaMoveRefusal::DestinationMissing));
    }
    if crate::platform::is_on_camera_card(destination) {
        return Ok(Err(MediaMoveRefusal::CameraCard));
    }
    let sources: Vec<SourceGroup> = source_groups(connection, ids)?
        .into_iter()
        .filter(|group| group.folder != destination)
        .collect();
    if sources.is_empty() {
        return Ok(Err(MediaMoveRefusal::NothingToMove));
    }
    let mut taken = HashSet::new();
    let mut groups = Vec::with_capacity(sources.len());
    for source in &sources {
        if source
            .media
            .iter()
            .any(|(_, name)| !source.folder.join(name).is_file())
        {
            return Ok(Err(MediaMoveRefusal::Missing));
        }
        if crate::platform::is_on_camera_card(&source.folder) {
            return Ok(Err(MediaMoveRefusal::CameraCard));
        }
        if !crate::platform::same_volume(&source.folder, destination) {
            return Ok(Err(MediaMoveRefusal::OtherDisk));
        }
        let stem = free_stem(
            connection,
            destination,
            &source.stem,
            &group_endings(source),
            &HashSet::new(),
            &mut taken,
        )?;
        groups.push(planned_group(source, destination, &stem));
    }
    Ok(Ok(MovePlan {
        kind: MediaMoveKind::Moved,
        groups,
    }))
}
