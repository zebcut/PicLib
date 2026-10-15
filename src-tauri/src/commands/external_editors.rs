//! Retouche dans un éditeur externe : liste des
//! éditeurs (préférences), « Modifier dans… », « Afficher dans le Finder ».

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use super::edits::{lock, media_ids_of};
use super::places::InterfaceLanguage;
use crate::catalog::Catalog;
use crate::catalog::repositories::library;
use crate::domain::{Language, MediaId};
use crate::external_editing::{
    self, CustomEditor, Editor, EditorSettings, SendError, SendRequest, application_name, custom_id,
};
use crate::platform;
use crate::settings;
use crate::workspace::Workspace;

/// Réglages des éditeurs en vigueur et fichier où ils sont enregistrés.
#[derive(Debug)]
pub struct AppEditors {
    settings_path: Option<PathBuf>,
    current: Mutex<EditorSettings>,
    /// Dossiers où chercher les applications.
    application_dirs: Vec<PathBuf>,
}

impl AppEditors {
    /// Lit les réglages enregistrés.
    pub fn load_for_app<R: Runtime>(app: &AppHandle<R>) -> Self {
        let settings_path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|config_dir| settings::settings_file(&config_dir));
        let current = settings_path
            .as_deref()
            .map(settings::load_editor_settings)
            .unwrap_or_default();
        Self {
            settings_path,
            current: Mutex::new(current),
            application_dirs: platform::application_dirs(app.path().home_dir().ok().as_deref()),
        }
    }

    fn settings(&self) -> EditorSettings {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn editors(&self) -> (Vec<Editor>, EditorSettings) {
        let settings = self.settings();
        (
            external_editing::available(&settings, &self.application_dirs),
            settings,
        )
    }

    /// Change les réglages et les enregistre.
    fn change(
        &self,
        change: impl FnOnce(&mut EditorSettings),
    ) -> Result<(), ExternalEditorsFailure> {
        let mut settings = self.settings();
        change(&mut settings);
        let path = self
            .settings_path
            .as_deref()
            .ok_or(ExternalEditorsFailure::SaveFailed)?;
        settings::save_editor_settings(path, &settings).map_err(|error| {
            tracing::error!(%error, "cannot save the external editors");
            ExternalEditorsFailure::SaveFailed
        })?;
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = settings;
        Ok(())
    }
}

/// Un éditeur, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditorDto {
    /// Identifiant.
    pub id: String,
    /// Nom affiché.
    pub name: String,
    /// Application ajoutée par l'utilisateur (elle peut être retirée).
    pub custom: bool,
}

/// Éditeurs proposés et éditeur de Cmd+E.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditorsDto {
    /// Éditeurs connus trouvés, puis applications ajoutées.
    pub editors: Vec<ExternalEditorDto>,
    /// Éditeur de Cmd+E ; `None` : aucun éditeur.
    pub default_id: Option<String>,
}

/// Échec d'un changement des éditeurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExternalEditorsFailure {
    /// Les réglages n'ont pas pu être enregistrés.
    SaveFailed,
    /// La fenêtre de choix a renvoyé autre chose qu'une application.
    NotAnApplication,
}

fn dto(editors: &[Editor], settings: &EditorSettings) -> ExternalEditorsDto {
    ExternalEditorsDto {
        editors: editors
            .iter()
            .map(|editor| ExternalEditorDto {
                id: editor.id.clone(),
                name: editor.name.clone(),
                custom: editor.preset.is_none(),
            })
            .collect(),
        default_id: external_editing::default_editor(editors, settings)
            .map(|editor| editor.id.clone()),
    }
}

fn current_dto(editors: &AppEditors) -> ExternalEditorsDto {
    let (found, settings) = editors.editors();
    dto(&found, &settings)
}

/// Éditeurs proposés (préférences, menu « Modifier dans »).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_external_editors(editors: State<'_, AppEditors>) -> ExternalEditorsDto {
    current_dto(&editors)
}

/// Fait choisir une application à ajouter aux éditeurs ; `None` si
/// l'utilisateur ferme la fenêtre sans choisir.
///
/// # Errors
///
/// Voir [`ExternalEditorsFailure`].
#[tauri::command]
#[specta::specta]
pub async fn add_external_editor(
    app: AppHandle,
    editors: State<'_, AppEditors>,
    title: String,
) -> Result<Option<ExternalEditorsDto>, ExternalEditorsFailure> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .add_filter("", &["app"]);
    if let Some(dir) = editors.application_dirs.first() {
        dialog = dialog.set_directory(dir);
    }
    let Some(chosen) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let application = chosen
        .into_path()
        .map_err(|_| ExternalEditorsFailure::NotAnApplication)?;
    if application
        .extension()
        .is_none_or(|extension| extension != "app")
    {
        return Err(ExternalEditorsFailure::NotAnApplication);
    }
    let id = custom_id(&application);
    editors.change(|settings| {
        if !settings
            .custom
            .iter()
            .any(|custom| custom.application == application)
        {
            settings.custom.push(CustomEditor {
                name: application_name(&application),
                application: application.clone(),
            });
        }
        // Une application qu'on vient d'ajouter est celle qu'on veut utiliser.
        settings.default_editor = Some(id);
    })?;
    Ok(Some(current_dto(&editors)))
}

/// Retire une application ajoutée par l'utilisateur.
///
/// # Errors
///
/// Voir [`ExternalEditorsFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn remove_external_editor(
    editors: State<'_, AppEditors>,
    id: String,
) -> Result<ExternalEditorsDto, ExternalEditorsFailure> {
    editors.change(|settings| {
        settings
            .custom
            .retain(|custom| custom_id(&custom.application) != id);
        if settings.default_editor.as_deref() == Some(id.as_str()) {
            settings.default_editor = None;
        }
    })?;
    Ok(current_dto(&editors))
}

/// Choisit l'éditeur de Cmd+E.
///
/// # Errors
///
/// Voir [`ExternalEditorsFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn set_default_external_editor(
    editors: State<'_, AppEditors>,
    id: String,
) -> Result<ExternalEditorsDto, ExternalEditorsFailure> {
    editors.change(|settings| settings.default_editor = Some(id))?;
    Ok(current_dto(&editors))
}

/// Ce que « Modifier dans… » ferait des photos choisies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditPreviewDto {
    /// Photos envoyées.
    pub photos: u32,
    /// Parmi elles, celles qui peuvent l'être par une copie (pas des RAW).
    pub copyable: u32,
    /// Vidéos et fichiers introuvables, écartés.
    pub skipped: u32,
    /// Éditeur qui a produit les versions traitées choisies (« Modifier de
    /// nouveau » les y renvoie), s'il est proposé ; `None` sinon.
    pub version_editor_id: Option<String>,
}

/// Prépare « Modifier dans… » : l'interface demande s'il faut confier
/// l'original ou une copie, et confirme au-delà de 10 photos.
///
/// # Errors
///
/// Voir [`ExternalEditFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn get_external_edit_preview(
    catalog: State<'_, Mutex<Catalog>>,
    editors: State<'_, AppEditors>,
    media_ids: Vec<u32>,
) -> Result<ExternalEditPreviewDto, ExternalEditFailure> {
    let plan = external_editing::plan(lock(&catalog).connection(), &media_ids_of(media_ids))
        .map_err(|error| {
            tracing::error!(%error, "cannot prepare the external edit");
            ExternalEditFailure::CatalogUnavailable
        })?;
    let (found, _) = editors.editors();
    Ok(ExternalEditPreviewDto {
        photos: count(plan.photos.len()),
        copyable: count(plan.copyable()),
        skipped: plan.videos + plan.missing,
        version_editor_id: plan
            .version_editor
            .filter(|id| found.iter().any(|editor| &editor.id == id)),
    })
}

/// Bilan de « Modifier dans… ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditOutcomeDto {
    /// Fichiers ouverts dans l'éditeur.
    pub opened: u32,
    /// Copies ajoutées au catalogue.
    pub copies: u32,
    /// Noms des photos dont la copie a échoué (non envoyées).
    pub copy_failed: Vec<String>,
    /// Vidéos et fichiers introuvables écartés.
    pub skipped: u32,
}

/// Échec de « Modifier dans… ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExternalEditFailure {
    /// Aucun catalogue n'est ouvert, ou il n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Aucun éditeur n'est installé ni ajouté (ou celui demandé a disparu).
    NoEditor,
    /// Aucune photo à envoyer (vidéos, fichiers introuvables, copies
    /// échouées).
    NothingToSend,
    /// L'éditeur n'a pas pu être lancé ; les copies faites restent.
    EditorNotOpened {
        /// Nom de l'éditeur.
        editor: String,
    },
}

/// Ouvre les photos `media_ids` dans l'éditeur `editor_id` (`None` :
/// celui de Cmd+E). `copy_non_raw` : confier par une copie `-Modifié` les
/// photos qui ne sont pas des RAW.
///
/// # Errors
///
/// Voir [`ExternalEditFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn edit_in_external_editor(
    app: AppHandle,
    editors: State<'_, AppEditors>,
    language: State<'_, InterfaceLanguage>,
    media_ids: Vec<u32>,
    editor_id: Option<String>,
    copy_non_raw: bool,
) -> Result<ExternalEditOutcomeDto, ExternalEditFailure> {
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(ExternalEditFailure::CatalogUnavailable)?;
    let (found, settings) = editors.editors();
    let ids = media_ids_of(media_ids);
    // Sans éditeur demandé : celui qui a produit les versions choisies,
    // sinon celui de Cmd+E.
    let version_editor = match editor_id {
        Some(_) => None,
        None => external_editing::plan(lock(&catalog).connection(), &ids)
            .ok()
            .and_then(|plan| plan.version_editor),
    };
    let editor = match editor_id.as_deref().or(version_editor.as_deref()) {
        Some(id) => found.iter().find(|editor| editor.id == id),
        None => None,
    }
    .or_else(|| {
        editor_id
            .is_none()
            .then(|| external_editing::default_editor(&found, &settings))
            .flatten()
    })
    .ok_or(ExternalEditFailure::NoEditor)?;
    let excluded = app
        .try_state::<Workspace>()
        .map(|workspace| workspace.internal_dirs())
        .unwrap_or_default();
    let request = SendRequest {
        editor,
        media_ids: &ids,
        copy_non_raw,
        copy_suffix: copy_suffix(language.get()),
        excluded: &excluded,
    };
    let result = external_editing::send(&catalog, &request);
    // Les photos envoyées sont désormais suivies.
    if let Some(watch) = app.try_state::<super::editor_watch::EditorWatch>() {
        watch.refresh();
    }
    if result
        .as_ref()
        .is_ok_and(|outcome| !outcome.copies.is_empty())
        || matches!(result, Err(SendError::Open(_)))
    {
        super::thumbnails::request_thumbnails(&app);
    }
    let outcome = result.map_err(|error| {
        tracing::warn!(%error, editor = %editor.id, "cannot send photos to the editor");
        match error {
            SendError::NothingToSend => ExternalEditFailure::NothingToSend,
            SendError::Catalog(_) | SendError::Import(_) => ExternalEditFailure::CatalogUnavailable,
            SendError::Open(_) => ExternalEditFailure::EditorNotOpened {
                editor: editor.name.clone(),
            },
        }
    })?;
    Ok(ExternalEditOutcomeDto {
        opened: count(outcome.opened),
        copies: count(outcome.copies.len()),
        copy_failed: outcome
            .copy_failed
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect(),
        skipped: outcome.skipped,
    })
}

/// Fin du nom des copies à retoucher, dans la langue de l'interface (`IMG_1234-Modifié.jpg`).
fn copy_suffix(language: Option<Language>) -> &'static str {
    match language {
        Some(Language::English) => "-Edited",
        Some(Language::French) | None => "-Modifié",
    }
}

/// Échec de « Afficher dans le Finder ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RevealFailure {
    /// Aucun catalogue n'est ouvert, ou le média n'y est plus.
    CatalogUnavailable,
    /// Le fichier est introuvable.
    FileMissing,
    /// Le système a refusé.
    Refused,
}

/// Montre le fichier du média `media_id` dans le Finder.
///
/// # Errors
///
/// Voir [`RevealFailure`].
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn reveal_in_finder(
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
) -> Result<(), RevealFailure> {
    let file = library::media_file(
        lock(&catalog).connection(),
        MediaId::new(i64::from(media_id)),
    )
    .map_err(|error| {
        tracing::error!(%error, "cannot read the file to reveal");
        RevealFailure::CatalogUnavailable
    })?
    .ok_or(RevealFailure::CatalogUnavailable)?;
    if !file.path.exists() {
        return Err(RevealFailure::FileMissing);
    }
    platform::reveal_in_file_manager(&file.path).map_err(|error| {
        tracing::warn!(%error, "cannot reveal the file");
        RevealFailure::Refused
    })
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
