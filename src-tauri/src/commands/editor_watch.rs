//! Retour des photos traitées par un éditeur externe : tant que PicLib est ouvert, les dossiers des photos envoyées sont
//! surveillés grâce aux avis du système (`FSEvents` sur macOS) ; un export de
//! l'éditeur y est ajouté au catalogue, empilé avec son original.
//!
//! Au lancement, et quand un dossier redevient accessible (disque
//! rebranché), il est relu une fois : ce qui a été exporté pendant que
//! PicLib était fermé est rattrapé.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use super::deletion::{TrashStarted, start_trash};
use super::edits::{lock, media_ids_of};
use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::catalog::repositories::{edited_versions, library};
use crate::domain::{MediaId, Timestamp};
use crate::external_editing::{
    self, FollowPlan, NewVersion, add_versions, follow_plan, known_media, mark_in_progress,
    new_versions,
};
use crate::files;
use crate::jobs::{JobContext, JobError};
use crate::workspace::Workspace;

/// Un fichier n'est examiné qu'après ce délai sans nouvel avis : l'éditeur a
/// fini de l'écrire.
const QUIET: Duration = Duration::from_secs(2);

/// Intervalle entre deux relectures de la liste des photos suivies (suivis
/// échus, disques rebranchés).
const REFRESH_EVERY: Duration = Duration::from_secs(30);

/// Attente maximale d'un avis avant de regarder les fichiers en attente.
const TICK: Duration = Duration::from_millis(500);

/// Message au fil de surveillance.
#[derive(Debug)]
enum Message {
    /// Relire la liste des photos suivies (une photo vient d'être envoyée).
    Refresh,
    /// Le système signale des changements.
    Changed(Vec<PathBuf>),
}

/// Accès au fil de surveillance.
#[derive(Debug, Default)]
pub struct EditorWatch(Mutex<Option<Sender<Message>>>);

impl EditorWatch {
    /// Demande de relire la liste des photos suivies.
    pub fn refresh(&self) {
        if let Some(sender) = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = sender.send(Message::Refresh);
        }
    }
}

/// Bilan d'un ajout de photos traitées.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditedVersionsSummary {
    /// Photos traitées ajoutées au catalogue.
    pub added: u32,
}

/// Lance le fil de surveillance et enregistre [`EditorWatch`] auprès de
/// l'application : à appeler une fois, au démarrage. L'état est enregistré
/// ici même, pour qu'il ne puisse pas manquer (Tauri arrête l'application
/// qui demande un état absent).
pub fn watch_edited_files(app: AppHandle) {
    let (sender, receiver) = mpsc::channel();
    app.manage(EditorWatch(Mutex::new(Some(sender.clone()))));
    let spawned = std::thread::Builder::new()
        .name("editor-watch".to_owned())
        .spawn(move || {
            let notify_sender = sender;
            let watcher = notify::recommended_watcher(
                move |event: notify::Result<notify::Event>| match event {
                    Ok(event) if is_relevant(event.kind) => {
                        let _ = notify_sender.send(Message::Changed(event.paths));
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "file system notification failed"),
                },
            );
            match watcher {
                Ok(watcher) => run(&app, watcher, &receiver),
                Err(error) => tracing::error!(%error, "cannot watch the edited files"),
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start watching the edited files");
    }
}

/// Avis qui peuvent annoncer un export : création, écriture, renommage.
fn is_relevant(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
    )
}

/// Boucle du fil : reçoit les avis, attend que les fichiers soient finis
/// d'écrire, relit la liste des photos suivies de temps en temps.
fn run(app: &AppHandle, mut watcher: RecommendedWatcher, receiver: &mpsc::Receiver<Message>) {
    let mut folders: BTreeSet<PathBuf> = BTreeSet::new();
    // Chemin réel d'un dossier surveillé ▸ chemin retenu par le catalogue :
    // le système signale les fichiers par leur chemin réel (`/private/var`
    // pour `/var`, liens symboliques résolus).
    let mut aliases: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut plan = FollowPlan::default();
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut next_refresh = Instant::now();
    loop {
        match receiver.recv_timeout(TICK) {
            Ok(Message::Refresh) => next_refresh = Instant::now(),
            Ok(Message::Changed(paths)) => {
                for path in paths {
                    let path = translated(&aliases, path);
                    // Un sous-dossier d'export qui apparaît (`darktable_exported`).
                    if plan.folders.contains_key(&path) && !folders.contains(&path) {
                        next_refresh = Instant::now();
                    }
                    pending.insert(path, Instant::now());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if Instant::now() >= next_refresh {
            if let Some(next) = read_plan(app) {
                plan = next;
                let added = follow_folders(&mut watcher, &mut folders, &plan);
                aliases = folders
                    .iter()
                    .filter_map(|folder| {
                        let real = folder.canonicalize().ok()?;
                        (&real != folder).then(|| (real, folder.clone()))
                    })
                    .collect();
                // Rattrape ce qui est arrivé sans que PicLib le voie.
                let long_ago = Instant::now()
                    .checked_sub(QUIET)
                    .unwrap_or_else(Instant::now);
                for folder in added {
                    for path in external_editing::files_in(&folder) {
                        pending.entry(path).or_insert(long_ago);
                    }
                }
            }
            next_refresh = Instant::now() + REFRESH_EVERY;
        }
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, seen)| seen.elapsed() >= QUIET)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &ready {
            pending.remove(path);
        }
        if !ready.is_empty() {
            add_found(app, &plan, &ready);
        }
    }
}

/// Chemin signalé par le système, exprimé comme le catalogue le connaît.
fn translated(aliases: &HashMap<PathBuf, PathBuf>, path: PathBuf) -> PathBuf {
    match (
        path.parent().and_then(|parent| aliases.get(parent)),
        path.file_name(),
    ) {
        (Some(folder), Some(name)) => folder.join(name),
        _ => path,
    }
}

/// Liste des photos suivies ; `None` sans catalogue ouvert.
fn read_plan(app: &AppHandle) -> Option<FollowPlan> {
    let catalog = app.try_state::<Mutex<Catalog>>()?;
    match follow_plan(&catalog, Timestamp::now()) {
        Ok(plan) => Some(plan),
        Err(error) => {
            tracing::warn!(%error, "cannot read the photos sent to an editor");
            None
        }
    }
}

/// Met la surveillance à jour d'après `plan` ; renvoie les dossiers qui
/// viennent d'être ajoutés.
fn follow_folders(
    watcher: &mut RecommendedWatcher,
    folders: &mut BTreeSet<PathBuf>,
    plan: &FollowPlan,
) -> Vec<PathBuf> {
    let wanted: BTreeSet<PathBuf> = plan
        .folders
        .keys()
        .filter(|folder| folder.is_dir())
        .cloned()
        .collect();
    for gone in folders.difference(&wanted).cloned().collect::<Vec<_>>() {
        // Un dossier disparu n'est déjà plus surveillé par le système.
        let _ = watcher.unwatch(&gone);
        folders.remove(&gone);
    }
    let mut added = Vec::new();
    for folder in wanted.difference(&folders.clone()) {
        match watcher.watch(folder, RecursiveMode::NonRecursive) {
            Ok(()) => {
                folders.insert(folder.clone());
                added.push(folder.clone());
            }
            Err(error) => {
                tracing::warn!(%error, folder = %folder.display(), "cannot watch a folder");
            }
        }
    }
    if !added.is_empty() {
        tracing::info!(
            folders = folders.len(),
            photos = plan.photo_count(),
            "watching the folders of the photos sent to an editor"
        );
    }
    added
}

/// Ajoute au catalogue, en tâche de fond, les exports parmi `paths`.
fn add_found(app: &AppHandle, plan: &FollowPlan, paths: &[PathBuf]) {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        return;
    };
    let found = {
        let catalog = lock(&catalog);
        let connection = catalog.connection();
        new_versions(connection, plan, paths)
            .and_then(|versions| Ok((versions, known_media(connection, paths)?)))
    };
    let (versions, known) = match found {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%error, "cannot recognize the edited files");
            return;
        }
    };
    // Un export réécrit, une photo modifiée telle quelle : nouvelle vignette.
    if !known.is_empty() {
        super::thumbnails::refresh_replaced(app, &known);
    }
    start_adding(app, versions);
}

/// Fichiers remis à PicLib par le système (« Exporter vers une application ▸ PicLib » d'un éditeur, « Ouvrir avec » du Finder) : un fichier du catalogue a sa vignette refaite ; un
/// export d'une photo suivie, ou dont le nom dérive d'une photo de son
/// dossier, devient une version traitée ; le reste ouvre l'écran d'import.
pub fn opened(app: &AppHandle, paths: &[PathBuf]) {
    let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
        super::import_requests::dropped(app, paths);
        return;
    };
    let plan = read_plan(app).unwrap_or_default();
    let found = {
        let catalog = lock(&catalog);
        let connection = catalog.connection();
        let read = || -> Result<_, crate::catalog::CatalogError> {
            let mut known = Vec::new();
            let mut others = Vec::new();
            for path in paths {
                let ids = known_media(connection, std::slice::from_ref(path))?;
                if ids.is_empty() {
                    others.push(path.clone());
                } else {
                    known.extend(ids);
                }
            }
            let mut versions = new_versions(connection, &plan, &others)?;
            let rest: Vec<PathBuf> = others
                .iter()
                .filter(|path| !versions.iter().any(|version| &version.path == *path))
                .cloned()
                .collect();
            versions.extend(external_editing::derived_versions(connection, &rest)?);
            let unknown: Vec<PathBuf> = rest
                .into_iter()
                .filter(|path| !versions.iter().any(|version| &version.path == path))
                .collect();
            Ok((known, versions, unknown))
        };
        read()
    };
    let (known, versions, unknown) = match found {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(%error, "cannot read the opened files");
            return;
        }
    };
    tracing::info!(
        files = paths.len(),
        known = known.len(),
        versions = versions.len(),
        "files opened with PicLib"
    );
    if !known.is_empty() {
        super::thumbnails::refresh_replaced(app, &known);
    }
    if !unknown.is_empty() {
        super::import_requests::dropped(app, &unknown);
    }
    start_adding(app, versions);
}

/// Ajoute `versions` au catalogue en tâche de fond.
fn start_adding(app: &AppHandle, versions: Vec<NewVersion>) {
    if versions.is_empty() {
        return;
    }
    let guard = mark_in_progress(
        &versions
            .iter()
            .map(|version| version.path.clone())
            .collect::<Vec<_>>(),
    );
    let kind = JobKind::EditedVersions {
        count: u32::try_from(versions.len()).unwrap_or(u32::MAX),
    };
    let handle = app.clone();
    let started = app.state::<AppJobManager>().start(kind, move |context| {
        let _guard = guard;
        run_add(&handle, &versions, context)
    });
    if let Err(error) = started {
        tracing::error!(%error, "cannot start adding the edited files");
    }
}

fn run_add(
    app: &AppHandle,
    versions: &[NewVersion],
    _context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;
    let excluded = app
        .try_state::<Workspace>()
        .map(|workspace| workspace.internal_dirs())
        .unwrap_or_default();
    let added = add_versions(&catalog, versions, &excluded).map_err(|error| {
        tracing::warn!(%error, "cannot add the edited files");
        JobError::Failed(JobFailure::CatalogError)
    })?;
    if let Err(error) =
        external_editing::touch(lock(&catalog).connection(), versions, Timestamp::now())
    {
        tracing::warn!(%error, "cannot extend the follow-up of the edited photos");
    }
    super::thumbnails::request_thumbnails(app);
    super::places::request_place_lookup(app);
    tracing::info!(added = added.len(), "edited files added to the catalog");
    Ok(JobResult::EditedVersions {
        summary: EditedVersionsSummary {
            added: u32::try_from(added.len()).unwrap_or(u32::MAX),
        },
    })
}

/// Photos suivies (Préférences ▸ Éditeurs externes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FollowedPhotosDto {
    /// Photos dont les exports sont attendus.
    pub photos: u32,
    /// Dossiers surveillés.
    pub folders: u32,
}

fn followed_of(plan: &FollowPlan) -> FollowedPhotosDto {
    FollowedPhotosDto {
        photos: u32::try_from(plan.photo_count()).unwrap_or(u32::MAX),
        folders: u32::try_from(plan.folders.len()).unwrap_or(u32::MAX),
    }
}

/// Échec d'une lecture ou d'un changement du suivi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FollowFailure {
    /// Aucun catalogue n'est ouvert, ou il n'a pas pu être lu ou écrit.
    CatalogUnavailable,
}

fn unavailable(error: &dyn std::fmt::Display) -> FollowFailure {
    tracing::error!(%error, "cannot read or change the followed photos");
    FollowFailure::CatalogUnavailable
}

/// Nombre de photos suivies et de dossiers surveillés.
///
/// # Errors
///
/// Voir [`FollowFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn get_followed_photos(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<FollowedPhotosDto, FollowFailure> {
    follow_plan(&catalog, Timestamp::now())
        .map(|plan| followed_of(&plan))
        .map_err(|error| unavailable(&error))
}

/// Arrête le suivi des photos `media_ids`, ou de toutes (`None`).
///
/// # Errors
///
/// Voir [`FollowFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn stop_following(
    catalog: State<'_, Mutex<Catalog>>,
    watch: State<'_, EditorWatch>,
    media_ids: Option<Vec<u32>>,
) -> Result<FollowedPhotosDto, FollowFailure> {
    let ids = media_ids.map(media_ids_of);
    external_editing::stop_following(lock(&catalog).connection(), ids.as_deref())
        .map_err(|error| unavailable(&error))?;
    let plan = follow_plan(&catalog, Timestamp::now()).map_err(|error| unavailable(&error))?;
    watch.refresh();
    Ok(followed_of(&plan))
}

/// Ce que le clic droit propose pour une photo envoyée à un éditeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditInfoDto {
    /// Ses exports sont attendus (« Ne plus suivre ses retouches »).
    pub followed: bool,
    /// Copie `-Modifié` apparemment restée identique à son original
    /// (« Mettre à la corbeille la copie non modifiée », qui le vérifie).
    pub unmodified_copy: bool,
}

/// Lit ce que le clic droit propose pour la photo `media_id`.
///
/// # Errors
///
/// Voir [`FollowFailure`].
#[tauri::command]
#[specta::specta]
pub async fn get_edit_info(
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
) -> Result<EditInfoDto, FollowFailure> {
    let id = MediaId::new(i64::from(media_id));
    let (followed, files) = {
        let catalog = lock(&catalog);
        let connection = catalog.connection();
        let followed: bool = connection
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM editor_sessions WHERE media_id = ?1)",
                [id.get()],
                |row| row.get(0),
            )
            .map_err(|error| unavailable(&error))?;
        let copy = edited_versions::get(connection, id)
            .map_err(|error| unavailable(&error))?
            .filter(|version| version.copy_digest.is_some());
        let files = match copy {
            Some(version) => {
                let file =
                    |id| library::media_file(connection, id).map_err(|error| unavailable(&error));
                file(id)?.zip(file(version.original_id)?)
            }
            None => None,
        };
        (followed, files)
    };
    Ok(EditInfoDto {
        followed,
        unmodified_copy: files
            .is_some_and(|(copy, original)| looks_unmodified(&copy.path, &original.path)),
    })
}

/// Vrai si la copie `copy` semble intacte : une copie vérifiée garde la
/// date et la taille de son original, et l'éditeur qui l'enregistre change
/// sa date. Rapide (le clic droit l'attend) ; l'empreinte est vérifiée
/// avant toute mise à la corbeille.
fn looks_unmodified(copy: &Path, original: &Path) -> bool {
    match (files::file_stamp(copy), files::file_stamp(original)) {
        (Ok(Some(copy)), Ok(Some(original))) => copy == original,
        _ => false,
    }
}

/// Vrai si le contenu de `path` a toujours l'empreinte `digest`.
fn is_unchanged(path: &Path, digest: &str) -> bool {
    files::file_digest(path).is_ok_and(|(_, found)| found.to_hex() == digest)
}

/// Échec de « Mettre à la corbeille la copie non modifiée ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrashCopyFailure {
    /// Aucun catalogue n'est ouvert, ou il n'a pas pu être lu.
    CatalogUnavailable,
    /// Ce n'est pas une copie `-Modifié`, ou elle a été modifiée depuis.
    NotUnmodified,
    /// La tâche de fond n'a pas pu être lancée.
    JobNotStarted,
}

/// Met à la corbeille la copie `-Modifié` `media_id`, si elle est toujours
/// identique à son original (vérifié de nouveau ici), puis la retire du
/// catalogue. Elle seule part, même en couverture d'une pile repliée (Suppr
/// emporterait toute la pile, original compris).
///
/// # Errors
///
/// Voir [`TrashCopyFailure`].
#[tauri::command]
#[specta::specta]
pub async fn trash_unmodified_copy(
    app: AppHandle,
    jobs: State<'_, AppJobManager>,
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
) -> Result<TrashStarted, TrashCopyFailure> {
    let id = MediaId::new(i64::from(media_id));
    let (digest, file) = {
        let catalog = lock(&catalog);
        let connection = catalog.connection();
        let read = || -> Result<_, crate::catalog::CatalogError> {
            Ok((
                edited_versions::get(connection, id)?.and_then(|version| version.copy_digest),
                library::media_file(connection, id)?,
            ))
        };
        read().map_err(|error| {
            tracing::error!(%error, "cannot read the copy to trash");
            TrashCopyFailure::CatalogUnavailable
        })?
    };
    match (digest, file) {
        (Some(digest), Some(file)) if is_unchanged(&file.path, &digest) => {}
        _ => return Err(TrashCopyFailure::NotUnmodified),
    }
    start_trash(app, &jobs, &catalog, &[id]).map_err(|failure| match failure {
        super::deletion::TrashStartFailure::CatalogUnavailable => {
            TrashCopyFailure::CatalogUnavailable
        }
        super::deletion::TrashStartFailure::JobNotStarted => TrashCopyFailure::JobNotStarted,
    })
}
