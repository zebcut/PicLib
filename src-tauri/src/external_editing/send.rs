//! « Modifier dans… » : les photos choisies sont
//! confiées à un éditeur externe. Un RAW s'ouvre tel quel (aucun éditeur
//! n'écrit dans un RAW) ; un JPEG, un TIFF, un PNG, un HEIC ou un PSD
//! s'ouvre tel quel ou par une copie vérifiée `-Modifié`, au choix de
//! l'utilisateur. Chaque photo envoyée est suivie : son dossier est
//! surveillé pour y reconnaître les exports de l'éditeur.
//!
//! « Modifier de nouveau » une version traitée envoie son
//! fichier de travail, jamais le simple tirage : le PSD (ou le TIFF, le
//! `.afphoto`) que l'éditeur a enregistré à côté s'il en garde un, sinon
//! le fichier qui lui avait été envoyé (le RAW, qui retrouve ses réglages
//! `.dop` ou `.xmp`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::editors::{Editor, Preset};
use super::versions::{NewVersion, add_versions, mark_in_progress};
use crate::catalog::repositories::edited_versions::{self, EditedVersion};
use crate::catalog::repositories::media;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{MediaFormat, MediaId, MediaKind, Timestamp};
use crate::files::copy_for_editing;
use crate::import::ImportError;
use crate::jobs::CancellationToken;
use crate::platform::{self, OpenError};

/// Photo qui peut être envoyée à un éditeur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditCandidate {
    /// Photo choisie.
    pub media_id: MediaId,
    /// Fichier ouvert dans l'éditeur : celui de la photo, ou le fichier de
    /// travail d'une version traitée.
    pub path: PathBuf,
    /// Format de la photo choisie.
    pub format: MediaFormat,
    /// Photo dont l'envoi est suivi (celle du fichier ouvert, ou celle dont
    /// le fichier de travail hors catalogue dépend).
    pub followed_id: MediaId,
    /// Version traitée : son fichier de travail est envoyé tel quel.
    pub is_version: bool,
}

impl EditCandidate {
    /// Vrai si la photo peut être confiée par une copie (tout sauf un RAW
    /// ou une version traitée).
    #[must_use]
    pub fn can_be_copied(&self) -> bool {
        !self.format.is_raw() && !self.is_version
    }
}

/// Ce que l'envoi de photos donnerait.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditPlan {
    /// Photos envoyées, dans l'ordre demandé.
    pub photos: Vec<EditCandidate>,
    /// Vidéos écartées (les éditeurs de photos ne les ouvrent pas).
    pub videos: u32,
    /// Photos dont le fichier est introuvable, écartées.
    pub missing: u32,
    /// Éditeur qui a produit les versions traitées choisies, s'il n'y en a
    /// qu'un : « Modifier de nouveau » les y renvoie.
    pub version_editor: Option<String>,
}

impl EditPlan {
    /// Photos qui peuvent être confiées par une copie.
    #[must_use]
    pub fn copyable(&self) -> usize {
        self.photos
            .iter()
            .filter(|photo| photo.can_be_copied())
            .count()
    }
}

/// Prépare l'envoi des médias `ids` (les identifiants inconnus sont
/// ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn plan(connection: &rusqlite::Connection, ids: &[MediaId]) -> Result<EditPlan, CatalogError> {
    let found = media::read_with_paths(connection, ids)?;
    let mut plan = EditPlan::default();
    let mut editors: Vec<Option<String>> = Vec::new();
    for id in ids {
        let Some((media, path)) = found.iter().find(|(media, _)| media.id == *id) else {
            continue;
        };
        if media.data.kind() == MediaKind::Video {
            plan.videos += 1;
            continue;
        }
        let version = edited_versions::get(connection, *id)?;
        let working = match &version {
            Some(version) => Some(working_file(connection, version, path)?),
            None => None,
        };
        let (open, followed_id) = working.clone().unwrap_or_else(|| (path.clone(), *id));
        if !open.is_file() {
            plan.missing += 1;
            continue;
        }
        if let Some(version) = &version
            && !editors.contains(&version.editor)
        {
            editors.push(version.editor.clone());
        }
        plan.photos.push(EditCandidate {
            media_id: *id,
            path: open,
            format: media.data.format,
            followed_id,
            is_version: working.is_some(),
        });
    }
    if let [Some(editor)] = editors.as_slice() {
        plan.version_editor = Some(editor.clone());
    }
    Ok(plan)
}

/// Fichier de travail d'une version traitée (dont le fichier est
/// `version_path`), et la photo dont l'envoi sera suivi.
fn working_file(
    connection: &rusqlite::Connection,
    version: &EditedVersion,
    version_path: &Path,
) -> Result<(PathBuf, MediaId), CatalogError> {
    let working_id = version.working_id.unwrap_or(version.original_id);
    let working = media::read_with_paths(connection, &[working_id])?
        .into_iter()
        .next()
        .map(|(_, path)| path);
    let original = media::read_with_paths(connection, &[version.original_id])?
        .into_iter()
        .next()
        .map(|(_, path)| path);
    // Le fichier à calques de l'éditeur, à côté de la version ou de son
    // original, au nom de l'original ou du fichier envoyé.
    let layered = version
        .editor
        .as_deref()
        .and_then(Preset::from_id)
        .map(Preset::working_extensions)
        .unwrap_or_default();
    let folders: Vec<&Path> = [Some(version_path), original.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(Path::parent)
        .collect();
    let stems: Vec<String> = [original.as_deref(), working.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(Path::file_stem)
        .map(|stem| stem.to_string_lossy().into_owned())
        .collect();
    for folder in &folders {
        for stem in &stems {
            for extension in layered {
                for spelled in [extension.to_string(), extension.to_uppercase()] {
                    let candidate = folder.join(format!("{stem}.{spelled}"));
                    if candidate.is_file() {
                        let id = media::find_in_folder_path(connection, &candidate)?
                            .unwrap_or(working_id);
                        return Ok((candidate, id));
                    }
                }
            }
        }
    }
    Ok(match working {
        Some(path) => (path, working_id),
        None => (version_path.to_path_buf(), version.media_id),
    })
}

/// Envoi demandé.
#[derive(Debug, Clone, Copy)]
pub struct SendRequest<'a> {
    /// Éditeur.
    pub editor: &'a Editor,
    /// Photos choisies.
    pub media_ids: &'a [MediaId],
    /// Confier par une copie les photos qui ne sont pas des RAW.
    pub copy_non_raw: bool,
    /// Fin du nom des copies (« -Modifié », dans la langue de
    /// l'interface).
    pub copy_suffix: &'a str,
    /// Dossiers internes au catalogue, jamais parcourus.
    pub excluded: &'a [PathBuf],
}

/// Bilan d'un envoi.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendOutcome {
    /// Fichiers ouverts dans l'éditeur.
    pub opened: usize,
    /// Copies ajoutées au catalogue.
    pub copies: Vec<MediaId>,
    /// Photos dont la copie a échoué (non envoyées).
    pub copy_failed: Vec<PathBuf>,
    /// Vidéos et fichiers introuvables écartés.
    pub skipped: u32,
}

/// Échec d'un envoi.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// Aucune photo ne peut être envoyée.
    #[error("nothing to send to the editor")]
    NothingToSend,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Les copies n'ont pas pu être ajoutées au catalogue.
    #[error(transparent)]
    Import(#[from] ImportError),
    /// L'éditeur n'a pas pu être lancé ; les copies faites restent.
    #[error(transparent)]
    Open(#[from] OpenError),
}

/// Envoie des photos à un éditeur : fait les copies demandées (ajoutées au
/// catalogue, empilées avec leur original), note les photos envoyées, puis
/// ouvre l'éditeur.
///
/// # Errors
///
/// Voir [`SendError`].
pub fn send(catalog: &Mutex<Catalog>, request: &SendRequest<'_>) -> Result<SendOutcome, SendError> {
    let plan = plan(lock(catalog).connection(), request.media_ids)?;
    let mut outcome = SendOutcome {
        skipped: plan.videos + plan.missing,
        ..SendOutcome::default()
    };
    let mut sent: Vec<(Option<MediaId>, PathBuf)> = Vec::new();
    let mut copies = Vec::new();
    // Chaque copie est écartée de la surveillance dès qu'elle a son nom :
    // sinon, pendant les copies suivantes, elle pourrait être prise pour un
    // export d'un envoi précédent de son original.
    let mut guards = Vec::new();
    for photo in &plan.photos {
        if !(request.copy_non_raw && photo.can_be_copied()) {
            sent.push((Some(photo.followed_id), photo.path.clone()));
            continue;
        }
        match copy_for_editing(
            &photo.path,
            request.copy_suffix,
            &CancellationToken::default(),
        ) {
            Ok((copy, digest)) => {
                guards.push(mark_in_progress(std::slice::from_ref(&copy)));
                copies.push(NewVersion {
                    path: copy,
                    original_id: photo.media_id,
                    editor: Some(request.editor.id.clone()),
                    working_id: None,
                    copy_digest: Some(digest.to_hex()),
                });
            }
            Err(error) => {
                tracing::warn!(path = %photo.path.display(), %error, "cannot copy a photo to edit");
                outcome.copy_failed.push(photo.path.clone());
            }
        }
    }
    if !copies.is_empty() {
        let added = add_versions(catalog, &copies, request.excluded)?;
        for copy in &copies {
            let id = added
                .iter()
                .find(|(_, version)| version.path == copy.path)
                .map(|(id, _)| *id);
            outcome.copies.extend(id);
            sent.push((id, copy.path.clone()));
        }
    }
    // Les copies sont au catalogue : la surveillance les reconnaît.
    drop(guards);
    if sent.is_empty() {
        return Err(SendError::NothingToSend);
    }
    {
        let mut catalog = lock(catalog);
        let transaction = catalog.transaction()?;
        let now = Timestamp::now();
        for id in sent.iter().filter_map(|(id, _)| *id) {
            edited_versions::record_sent(&transaction, id, &request.editor.id, now)?;
        }
        transaction.commit().map_err(CatalogError::from)?;
    }
    let paths: Vec<PathBuf> = sent.into_iter().map(|(_, path)| path).collect();
    platform::open_with_application(&request.editor.application, &paths)?;
    tracing::info!(
        editor = %request.editor.id,
        files = paths.len(),
        copies = outcome.copies.len(),
        "photos sent to an external editor"
    );
    outcome.opened = paths.len();
    Ok(outcome)
}

fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
