//! Modifier la date de prise de vue : une date saisie, ou le
//! retour à la date d'origine, de façon annulable.

use std::sync::Mutex;

use serde::Deserialize;
use specta::Type;
use tauri::State;

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use crate::catalog::Catalog;
use crate::domain::CaptureTime;
use crate::edits::{CaptureTimeChange, CaptureTimeError, Edit, History};

/// Changement de date, tel que demandé par l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureTimeChangeDto {
    /// Donner cette date à tous les médias.
    Set {
        /// Date locale, `2024-07-14T18:30:05` (les secondes peuvent manquer).
        local: String,
    },
    /// Revenir à la date lue à l'import.
    Original,
}

/// Change la date de prise de vue des médias indiqués, de façon annulable.
///
/// # Errors
///
/// [`EditFailure::InvalidValue`] si la date est illisible ou hors de l'an 1
/// à l'an 9999 ; [`EditFailure::CatalogUnavailable`] si le catalogue est
/// illisible. Rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn change_capture_time(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    change: CaptureTimeChangeDto,
) -> Result<EditOutcome, EditFailure> {
    let change = match change {
        CaptureTimeChangeDto::Set { local } => {
            CaptureTimeChange::Set(parse_input(&local).ok_or(EditFailure::InvalidValue)?)
        }
        CaptureTimeChangeDto::Original => CaptureTimeChange::Original,
    };
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let edit = Edit::capture_time(&catalog, change, &ids).map_err(|error| match error {
        CaptureTimeError::Catalog(error) => failure(&error),
        CaptureTimeError::OutOfRange => EditFailure::InvalidValue,
    })?;
    let summary = history
        .perform(&mut catalog, edit)
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Date saisie dans l'interface : avec ou sans les secondes (le champ date
/// et heure du système les omet quand elles valent zéro).
fn parse_input(text: &str) -> Option<jiff::civil::DateTime> {
    CaptureTime::parse_storage_text(text)
        .or_else(|_| jiff::civil::DateTime::strptime("%Y-%m-%dT%H:%M", text))
        .ok()
}
