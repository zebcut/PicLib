//! Ce qui ouvre l'écran d'import sans passer par le menu, et la fin d'un
//! import depuis une carte :
//!
//! - une carte mémoire branchée, si la préférence est cochée ;
//! - un dossier (ou des fichiers) glissé du Finder sur la fenêtre ;
//! - l'éjection de la carte, proposée par le bilan.
//!
//! L'interface s'abonne une fois aux demandes d'ouverture par un canal dédié
//! (comme pour les tâches) ; les chemins restent dans le cœur : une demande
//! désigne une source par son numéro.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use super::import_sources::ImportSources;
use super::preferences::AppPreferences;
use crate::catalog::Catalog;
use crate::domain::MediaFormat;
use crate::platform;

/// Intervalle entre deux relectures des disques montés.
const CARD_WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// Demande d'ouverture de l'écran d'import sur une source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequestDto {
    /// Source à analyser.
    pub source_id: u32,
    /// Fichiers glissés, à la racine de la source : seuls ceux-là sont
    /// cochés ; vide : les nouveaux fichiers, comme d'habitude.
    pub only_names: Vec<String>,
}

/// Canal vers l'interface, une fois abonnée, et les demandes arrivées avant
///(photo ouverte avec PicLib qui le lance).
#[derive(Default)]
pub struct ImportRequests(Mutex<(Option<Channel<ImportRequestDto>>, Vec<ImportRequestDto>)>);

impl std::fmt::Debug for ImportRequests {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ImportRequests")
    }
}

impl ImportRequests {
    fn send(&self, request: ImportRequestDto) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match state.0.as_ref() {
            Some(channel) => {
                if let Err(error) = channel.send(request) {
                    tracing::warn!(%error, "cannot send an import request");
                }
            }
            None => state.1.push(request),
        }
    }
}

/// Abonne l'interface aux demandes d'ouverture de l'écran d'import.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn subscribe_import_requests(
    requests: State<'_, ImportRequests>,
    channel: Channel<ImportRequestDto>,
) {
    let mut state = requests.0.lock().unwrap_or_else(PoisonError::into_inner);
    for request in std::mem::take(&mut state.1) {
        if let Err(error) = channel.send(request) {
            tracing::warn!(%error, "cannot send an import request");
        }
    }
    state.0 = Some(channel);
}

/// Nom affiché d'un disque.
#[must_use]
pub fn volume_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Ouvre l'écran d'import sur `path`, s'il est permis de le proposer (un
/// catalogue est ouvert).
fn request(app: &AppHandle, path: &Path, only_names: Vec<String>) {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return;
    }
    let (Some(sources), Some(requests)) = (
        app.try_state::<ImportSources>(),
        app.try_state::<ImportRequests>(),
    ) else {
        return;
    };
    requests.send(ImportRequestDto {
        source_id: sources.register(path),
        only_names,
    });
}

/// Surveille les disques montés : une carte mémoire branchée ouvre l'écran
/// d'import si la préférence est cochée (décochée par défaut). Les cartes
/// déjà branchées au démarrage ne l'ouvrent pas.
pub fn watch_cards(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("card-watcher".to_owned())
        .spawn(move || {
            let mut known = mounted_cards();
            loop {
                std::thread::sleep(CARD_WATCH_INTERVAL);
                let cards = mounted_cards();
                let wanted = app
                    .try_state::<AppPreferences>()
                    .is_some_and(|preferences| preferences.current().open_import_on_card);
                if wanted {
                    for card in cards.difference(&known) {
                        tracing::info!(card = %card.display(), "camera card connected");
                        request(&app, card, Vec::new());
                    }
                }
                known = cards;
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot watch for camera cards");
    }
}

fn mounted_cards() -> HashSet<PathBuf> {
    platform::mounted_volumes()
        .into_iter()
        .map(|volume| volume.root)
        .filter(|root| platform::is_on_camera_card(root))
        .collect()
}

/// Source et fichiers à cocher pour ce qui a été glissé du Finder : un
/// dossier devient la source ; des fichiers, leur dossier, avec seulement
/// eux de cochés (les photos et vidéos du même dossier que le premier).
#[must_use]
pub fn dropped_source(paths: &[PathBuf]) -> Option<(PathBuf, Vec<String>)> {
    let first = paths.first()?;
    if first.is_dir() {
        return Some((first.clone(), Vec::new()));
    }
    let folder = first.parent()?.to_path_buf();
    let names: Vec<String> = paths
        .iter()
        .filter(|path| path.parent() == Some(folder.as_path()))
        .filter(|path| MediaFormat::from_path(path).is_some())
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    (!names.is_empty()).then_some((folder, names))
}

/// Ce qui a été glissé du Finder sur la fenêtre : l'écran d'import s'ouvre
/// dessus.
pub fn dropped(app: &AppHandle, paths: &[PathBuf]) {
    if let Some((folder, names)) = dropped_source(paths) {
        super::import_sources::remember_folder(app, &folder);
        request(app, &folder, names);
    } else {
        tracing::debug!(?paths, "nothing to import in the dropped items");
    }
}

/// Cartes mémoire importées, par tâche d'import : le bilan propose de les
/// éjecter.
#[derive(Debug, Default)]
pub struct ImportedCards(Mutex<HashMap<u32, PathBuf>>);

/// Retient la carte d'un import, à éjecter à la fin.
pub fn remember_card(app: &AppHandle, import_job_id: u32, root: PathBuf) {
    if let Some(cards) = app.try_state::<ImportedCards>() {
        cards
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(import_job_id, root);
    }
}

/// Raison pour laquelle la carte n'a pas été éjectée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EjectFailure {
    /// Cet import ne venait pas d'une carte, ou elle est déjà éjectée.
    UnknownCard,
    /// Le système a refusé (un programme l'utilise encore).
    Refused,
}

/// Éjecte la carte mémoire de l'import `import_job_id` : elle
/// peut ensuite être retirée. Rien n'y est écrit.
///
/// # Errors
///
/// Voir [`EjectFailure`].
#[tauri::command]
#[specta::specta]
pub async fn eject_card(
    cards: State<'_, ImportedCards>,
    import_job_id: u32,
) -> Result<(), EjectFailure> {
    let root = cards
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&import_job_id)
        .cloned()
        .ok_or(EjectFailure::UnknownCard)?;
    platform::eject(&root).map_err(|error| {
        tracing::warn!(%error, card = %root.display(), "cannot eject the card");
        EjectFailure::Refused
    })?;
    tracing::info!(card = %root.display(), "card ejected");
    cards
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&import_job_id);
    Ok(())
}
