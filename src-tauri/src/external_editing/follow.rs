//! Photos suivies après leur envoi à un éditeur :
//! quels dossiers surveiller, et quel fichier y est un export de l'éditeur.
//!
//! Un fichier est reconnu comme version traitée d'une photo suivie s'il est
//! dans son dossier (ou dans le sous-dossier où l'éditeur range ses
//! exports), si c'est une photo d'un format pris en charge que le catalogue
//! ne connaît pas, si son nom commence par celui de la photo (`DSC_1234`
//! pour `DSC_1234_DxO.jpg`, `DSC_1234-Edit.tif`, `DSC_1234.psd`) et s'il a
//! été écrit après l'envoi. Le suivi d'une photo s'arrête 30 jours après le
//! dernier envoi ou le dernier export reçu, quand elle quitte le catalogue ou quand son dossier
//! disparaît d'un disque branché.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, params};

use super::editors::Preset;
use super::versions::{NewVersion, is_in_progress};
use crate::catalog::repositories::{folders, media};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaFormat, MediaId, MediaKind, Timestamp};
use crate::files::{file_stamp, list_folder};
use crate::platform;

/// Durée du suivi après la dernière activité, en millisecondes (30 jours).
pub const FOLLOW_DURATION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Marge sur la date d'un export, en millisecondes : les systèmes de
/// fichiers arrondissent les dates.
const DATE_TOLERANCE_MS: i64 = 2000;

/// Photo suivie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowedPhoto {
    /// Photo envoyée (le RAW, ou la copie `-Modifié`).
    pub media_id: MediaId,
    /// Éditeur qui l'a reçue.
    pub editor: String,
    /// Son nom sans l'extension.
    pub stem: String,
    /// Son nom complet.
    pub file_name: String,
    /// Premier envoi : les fichiers plus anciens ne sont pas des exports.
    pub sent_at: Timestamp,
    /// Original des versions qui en sortent (lui-même, ou l'original d'une
    /// copie).
    pub original_id: MediaId,
}

/// Dossiers à surveiller et photos suivies dans chacun.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FollowPlan {
    /// Dossier ▸ photos dont il peut recevoir les exports.
    pub folders: BTreeMap<PathBuf, Vec<FollowedPhoto>>,
}

impl FollowPlan {
    /// Nombre de photos suivies.
    #[must_use]
    pub fn photo_count(&self) -> usize {
        let mut ids: Vec<MediaId> = self
            .folders
            .values()
            .flatten()
            .map(|photo| photo.media_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// Photo suivie dont `path`, écrit à `modified`, est un export ; `None`
    /// si ce n'en est pas un (le catalogue n'est pas consulté).
    #[must_use]
    pub fn recognize(&self, path: &Path, modified: Timestamp) -> Option<&FollowedPhoto> {
        let photos = self.folders.get(path.parent()?)?;
        let name = path.file_name()?.to_str()?;
        if name.starts_with('.')
            || MediaFormat::from_path(path).is_none_or(|format| format.kind() != MediaKind::Photo)
        {
            return None;
        }
        let lower = name.to_lowercase();
        photos
            .iter()
            .filter(|photo| {
                lower != photo.file_name.to_lowercase()
                    && derives_from(&lower, &photo.stem.to_lowercase())
                    && modified.unix_millis() + DATE_TOLERANCE_MS >= photo.sent_at.unix_millis()
            })
            // `DSC_1234-2` plutôt que `DSC_1234` pour `DSC_1234-2_DxO.jpg`.
            .max_by_key(|photo| photo.stem.len())
    }
}

/// Vrai si le nom `name` (minuscules) commence par `stem` suivi d'autre
/// chose qu'une lettre ou un chiffre : `dsc_1234_dxo.jpg` vient de
/// `dsc_1234`, pas `dsc_12345.jpg`.
fn derives_from(name: &str, stem: &str) -> bool {
    name.strip_prefix(stem)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| !next.is_alphanumeric())
}

/// Lit les photos suivies et les dossiers à surveiller ; oublie d'abord les
/// suivis échus (30 jours sans activité) et ceux dont le dossier a
/// disparu d'un disque branché. Le catalogue n'est verrouillé que pour le
/// lire et l'écrire : les dossiers sont examinés hors du verrou (un disque
/// lent ou en veille ne bloque pas le reste de l'application).
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn plan(catalog: &Mutex<Catalog>, now: Timestamp) -> Result<FollowPlan, CatalogError> {
    let rows = followed_rows(lock(catalog).connection(), now)?;
    let mut plan = FollowPlan::default();
    let mut gone = Vec::new();
    let startup = platform::startup_disk();
    for row in rows {
        let folder = PathBuf::from(&row.folder);
        if !folder.is_dir() {
            // Disque débranché : le suivi reprendra à son retour.
            if platform::is_disk_online(&platform::disk_of(&folder, &startup)) {
                gone.push((row.media_id, row.editor));
            }
            continue;
        }
        let stem = Path::new(&row.file_name)
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let photo = FollowedPhoto {
            media_id: MediaId::new(row.media_id),
            stem,
            file_name: row.file_name,
            sent_at: Timestamp::from_unix_millis(row.sent_at),
            original_id: MediaId::new(row.original_id),
            editor: row.editor,
        };
        if let Some(subfolder) = Preset::from_id(&photo.editor).and_then(Preset::export_subfolder) {
            plan.folders
                .entry(folder.join(subfolder))
                .or_default()
                .push(photo.clone());
        }
        plan.folders.entry(folder).or_default().push(photo);
    }
    if !gone.is_empty() {
        let catalog = lock(catalog);
        let mut forget = catalog
            .connection()
            .prepare_cached("DELETE FROM editor_sessions WHERE media_id = ?1 AND editor = ?2")?;
        for (media_id, editor) in gone {
            forget.execute(params![media_id, editor])?;
        }
    }
    Ok(plan)
}

/// Photo suivie, telle que lue au catalogue.
struct FollowedRow {
    media_id: i64,
    editor: String,
    sent_at: i64,
    folder: String,
    file_name: String,
    original_id: i64,
}

/// Oublie les suivis échus et lit les autres.
fn followed_rows(
    connection: &Connection,
    now: Timestamp,
) -> Result<Vec<FollowedRow>, CatalogError> {
    connection
        .prepare_cached("DELETE FROM editor_sessions WHERE active_at < ?1")?
        .execute([now.unix_millis() - FOLLOW_DURATION_MS])?;
    let mut statement = connection.prepare_cached(
        "SELECT editor_sessions.media_id, editor_sessions.editor, editor_sessions.sent_at,
                folders.path, media.file_name,
                coalesce(edited_versions.original_id, editor_sessions.media_id)
         FROM editor_sessions
         JOIN media ON media.id = editor_sessions.media_id
         JOIN folders ON folders.id = media.folder_id
         LEFT JOIN edited_versions ON edited_versions.media_id = editor_sessions.media_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(FollowedRow {
            media_id: row.get(0)?,
            editor: row.get(1)?,
            sent_at: row.get(2)?,
            folder: row.get(3)?,
            file_name: row.get(4)?,
            original_id: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Versions traitées à ajouter parmi les fichiers `paths` (signalés par le
/// système, ou trouvés en relisant un dossier) : ceux que `plan`
/// reconnaît et que le catalogue ne connaît pas encore.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn new_versions(
    connection: &Connection,
    plan: &FollowPlan,
    paths: &[PathBuf],
) -> Result<Vec<NewVersion>, CatalogError> {
    let mut found: Vec<NewVersion> = Vec::new();
    for path in paths {
        if found.iter().any(|version| &version.path == path) || is_in_progress(path) {
            continue;
        }
        // Disparu entre-temps, ou dossier : pas un export.
        let Ok(Some(stamp)) = file_stamp(path) else {
            continue;
        };
        if path.is_dir() {
            continue;
        }
        let Some(photo) = plan.recognize(path, stamp.modified) else {
            continue;
        };
        if is_known(connection, path)? {
            continue;
        }
        found.push(NewVersion {
            path: path.clone(),
            original_id: photo.original_id,
            editor: Some(photo.editor.clone()),
            working_id: Some(photo.media_id),
            copy_digest: None,
        });
    }
    Ok(found)
}

/// Versions traitées parmi des fichiers remis à PicLib par un éditeur
///(« Exporter vers une application ▸ PicLib »), sans envoi
/// préalable : un fichier inconnu du catalogue, rangé dans le dossier d'une
/// photo du catalogue (ou dans un sous-dossier d'export connu) et dont le
/// nom dérive du sien. L'éditeur est inconnu ; la photo trouvée est le
/// fichier de travail.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn derived_versions(
    connection: &Connection,
    paths: &[PathBuf],
) -> Result<Vec<NewVersion>, CatalogError> {
    let subfolders: Vec<&str> = Preset::ALL
        .into_iter()
        .filter_map(Preset::export_subfolder)
        .collect();
    let mut found = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.')
            || MediaFormat::from_path(path).is_none_or(|format| format.kind() != MediaKind::Photo)
            || is_known(connection, path)?
            || is_in_progress(path)
        {
            continue;
        }
        let Some(folder) = path.parent() else {
            continue;
        };
        // Le dossier lui-même, ou celui au-dessus d'un sous-dossier d'export.
        let mut candidates = vec![folder];
        if folder
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| subfolders.contains(&name))
            && let Some(parent) = folder.parent()
        {
            candidates.push(parent);
        }
        let lower = name.to_lowercase();
        let mut best: Option<(usize, MediaId)> = None;
        for candidate in candidates {
            let Some(folder) = folders::find_by_path(connection, candidate)? else {
                continue;
            };
            for media in media::list_in_folder(connection, folder.id)? {
                let stem = Path::new(&media.data.file_name)
                    .file_stem()
                    .map_or_else(String::new, |stem| stem.to_string_lossy().to_lowercase());
                if !stem.is_empty()
                    && derives_from(&lower, &stem)
                    && best.is_none_or(|(length, _)| stem.len() > length)
                {
                    best = Some((stem.len(), media.id));
                }
            }
        }
        if let Some((_, working)) = best {
            let original: Option<i64> = connection
                .prepare_cached("SELECT original_id FROM edited_versions WHERE media_id = ?1")?
                .query_row([working.get()], |row| row.get(0))
                .ok();
            found.push(NewVersion {
                path: path.clone(),
                original_id: original.map_or(working, MediaId::new),
                editor: None,
                working_id: Some(working),
                copy_digest: None,
            });
        }
    }
    Ok(found)
}

/// Médias du catalogue parmi les fichiers `paths` : leur fichier a pu être
/// réécrit par l'éditeur.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn known_media(
    connection: &Connection,
    paths: &[PathBuf],
) -> Result<Vec<MediaId>, CatalogError> {
    let mut found = Vec::new();
    for path in paths {
        let (Some(folder), Some(name)) = (
            path.parent(),
            path.file_name().and_then(|name| name.to_str()),
        ) else {
            continue;
        };
        if let Some(folder) = folders::find_by_path(connection, folder)?
            && let Some(media) = media::find_in_folder(connection, folder.id, name)?
            && !found.contains(&media.id)
        {
            found.push(media.id);
        }
    }
    Ok(found)
}

/// Fichiers des dossiers surveillés, pour rattraper ce qui a été exporté
/// pendant que PicLib était fermé (ou le disque débranché).
#[must_use]
pub fn files_in(folder: &Path) -> Vec<PathBuf> {
    list_folder(folder)
        .map(|report| report.files.into_iter().map(|file| file.path).collect())
        .unwrap_or_default()
}

/// Prolonge le suivi des photos qui viennent de donner un export.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn touch(
    connection: &Connection,
    versions: &[NewVersion],
    now: Timestamp,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "UPDATE editor_sessions SET active_at = ?1 WHERE media_id = ?2 AND editor = ?3",
    )?;
    for version in versions {
        if let (Some(working), Some(editor)) = (version.working_id, &version.editor) {
            statement.execute(params![now.unix_millis(), working.get(), editor])?;
        }
    }
    Ok(())
}

/// Arrête le suivi des photos `ids` (« Ne plus suivre ses retouches »), ou
/// de toutes si `ids` est `None` (Préférences ▸ « Arrêter le suivi »).
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn stop(connection: &Connection, ids: Option<&[MediaId]>) -> Result<usize, CatalogError> {
    Ok(match ids {
        None => connection.execute("DELETE FROM editor_sessions", [])?,
        Some(ids) => {
            let mut statement =
                connection.prepare_cached("DELETE FROM editor_sessions WHERE media_id = ?1")?;
            let mut removed = 0;
            for id in ids {
                removed += statement.execute([id.get()])?;
            }
            removed
        }
    })
}

fn is_known(connection: &Connection, path: &Path) -> Result<bool, CatalogError> {
    let (Some(folder), Some(name)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Ok(true);
    };
    Ok(match folders::find_by_path(connection, folder)? {
        Some(folder) => media::exists_in_folder(connection, folder.id, name)?,
        None => false,
    })
}
