//! Tâche de fond qui remplit le lieu des photos géolocalisées, et langue de
//! l'interface dans laquelle ce lieu est écrit.
//!
//! Le cœur ne connaît pas la langue de l'interface : elle la lui annonce au
//! démarrage, et cette annonce lance le premier passage. Chaque import en
//! relance un pour ses nouvelles photos.

use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use super::jobs::{AppJobManager, JobFailure, JobKind, JobResult};
use crate::catalog::Catalog;
use crate::domain::Language;
use crate::jobs::{JobContext, JobError, PassScheduler};
use crate::places::{FillError, fill_pending};

/// Langue de l'interface, telle qu'échangée avec elle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
pub enum InterfaceLanguageDto {
    /// Français.
    #[serde(rename = "fr")]
    French,
    /// Anglais.
    #[serde(rename = "en")]
    English,
}

impl From<InterfaceLanguageDto> for Language {
    fn from(language: InterfaceLanguageDto) -> Self {
        match language {
            InterfaceLanguageDto::French => Self::French,
            InterfaceLanguageDto::English => Self::English,
        }
    }
}

/// Langue annoncée par l'interface ; `None` tant qu'elle ne l'a pas fait.
#[derive(Debug, Default)]
pub struct InterfaceLanguage(Mutex<Option<Language>>);

impl InterfaceLanguage {
    pub(super) fn get(&self) -> Option<Language> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, language: Language) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(language);
    }
}

/// Déclenchement de la tâche des lieux (voir [`PassScheduler`]).
#[derive(Debug, Default)]
pub struct PlaceScheduler(PassScheduler);

/// Bilan d'un remplissage des lieux, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlacesSummary {
    /// Photos dont au moins un champ (pays, région, ville) a été rempli.
    #[specta(type = u32)]
    pub filled: u64,
}

/// Annonce la langue de l'interface, puis remplit le lieu des photos
/// géolocalisées qui ne l'ont pas encore été.
// Tauri ne sait passer `AppHandle` à une commande que par valeur.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn set_interface_language(app: AppHandle, language: InterfaceLanguageDto) {
    app.state::<InterfaceLanguage>().set(language.into());
    request_place_lookup(&app);
}

/// Lance le remplissage des lieux, sauf s'il tourne déjà (il refera alors
/// un passage). Sans catalogue ouvert, ou tant que l'interface n'a pas
/// annoncé sa langue, ne fait rien : l'annonce lancera le premier passage.
pub fn request_place_lookup(app: &AppHandle) {
    if app.try_state::<Mutex<Catalog>>().is_none() {
        return;
    }
    let Some(language) = app.state::<InterfaceLanguage>().get() else {
        return;
    };
    let scheduler = app.state::<PlaceScheduler>();
    if !scheduler.0.begin() {
        return;
    }
    let handle = app.clone();
    let started = app
        .state::<AppJobManager>()
        .start(JobKind::Places, move |context| {
            run_lookup(&handle, language, context)
        });
    if let Err(error) = started {
        tracing::error!(%error, "cannot start the place lookup");
        scheduler.0.end();
    }
}

/// Travail de la tâche : passages successifs tant que de nouvelles photos
/// sont arrivées entre-temps.
fn run_lookup(
    app: &AppHandle,
    language: Language,
    context: &JobContext<JobKind, JobResult, JobFailure>,
) -> Result<JobResult, JobError<JobFailure>> {
    let scheduler = app.state::<PlaceScheduler>();
    let _end = scheduler.0.end_guard();
    let catalog = app
        .try_state::<Mutex<Catalog>>()
        .ok_or(JobError::Failed(JobFailure::CatalogError))?;

    let mut summary = PlacesSummary { filled: 0 };
    loop {
        let report = fill_pending(&catalog, language, context.cancellation(), |done, total| {
            context.report_progress_of(done, total);
        })
        .map_err(|error| match error {
            FillError::Cancelled => JobError::Cancelled,
            FillError::Catalog(error) => {
                tracing::warn!(%error, "place lookup stopped");
                JobError::Failed(JobFailure::CatalogError)
            }
            // Base intégrée mal formée : erreur de fabrication, qu'un test
            // vérifie. Rien n'a été écrit ; la tâche, discrète, s'arrête.
            FillError::Data(error) => {
                tracing::error!(%error, "the embedded place data is invalid");
                JobError::Failed(JobFailure::CatalogError)
            }
        })?;
        summary.filled += report.filled;
        tracing::info!(?report, "place lookup pass finished");
        if !scheduler.0.pass_finished() {
            break;
        }
    }
    Ok(JobResult::Places { summary })
}
