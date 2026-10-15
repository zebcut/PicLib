//! Préréglages de filtres de l'utilisateur :
//! créer, mettre à jour, renommer, supprimer, chaque fois de façon
//! annulable (Cmd+Z). Le contenu est fourni et vérifié par les commandes.

use unicode_normalization::UnicodeNormalization;

use crate::catalog::repositories::filter_presets::{self, FilterPreset};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{FilterPresetId, MAX_KEYWORD_CHARS};
use crate::edits::{Edit, EditSummary, History, PresetChange, PresetEdit};

/// Changement de préréglage refusé.
#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    /// Nom vide.
    #[error("empty preset name")]
    EmptyName,
    /// Nom trop long.
    #[error("preset name too long")]
    TooLong,
    /// Un autre préréglage porte déjà ce nom.
    #[error("preset name {0:?} already used")]
    NameTaken(String),
    /// Le préréglage n'existe pas (ou plus).
    #[error("preset not found")]
    NotFound,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Crée un préréglage nommé `name`, de contenu `content`.
///
/// # Errors
///
/// Voir [`PresetError`] ; rien n'est créé en cas d'échec.
pub fn create(
    catalog: &mut Catalog,
    history: &mut History,
    name: &str,
    content: String,
) -> Result<(FilterPresetId, EditSummary), PresetError> {
    let name = free_name(catalog, name, None)?;
    let id = filter_presets::next_id(catalog.connection())?;
    let preset = FilterPreset { id, name, content };
    let summary = history.perform(catalog, Edit::FilterPreset(PresetEdit::created(preset)))?;
    Ok((id, summary))
}

/// Remplace le contenu d'un préréglage.
///
/// # Errors
///
/// Voir [`PresetError`].
pub fn update(
    catalog: &mut Catalog,
    history: &mut History,
    id: FilterPresetId,
    content: String,
) -> Result<EditSummary, PresetError> {
    let before = existing(catalog, id)?;
    let after = FilterPreset {
        content,
        ..before.clone()
    };
    perform(
        catalog,
        history,
        PresetEdit::changed(PresetChange::Updated, before, after),
    )
}

/// Renomme un préréglage.
///
/// # Errors
///
/// Voir [`PresetError`].
pub fn rename(
    catalog: &mut Catalog,
    history: &mut History,
    id: FilterPresetId,
    name: &str,
) -> Result<EditSummary, PresetError> {
    let before = existing(catalog, id)?;
    let name = free_name(catalog, name, Some(id))?;
    let after = FilterPreset {
        name,
        ..before.clone()
    };
    perform(
        catalog,
        history,
        PresetEdit::changed(PresetChange::Renamed, before, after),
    )
}

/// Supprime un préréglage.
///
/// # Errors
///
/// Voir [`PresetError`].
pub fn delete(
    catalog: &mut Catalog,
    history: &mut History,
    id: FilterPresetId,
) -> Result<EditSummary, PresetError> {
    let preset = existing(catalog, id)?;
    perform(catalog, history, PresetEdit::deleted(preset))
}

fn perform(
    catalog: &mut Catalog,
    history: &mut History,
    edit: PresetEdit,
) -> Result<EditSummary, PresetError> {
    Ok(history.perform(catalog, Edit::FilterPreset(edit))?)
}

fn existing(catalog: &Catalog, id: FilterPresetId) -> Result<FilterPreset, PresetError> {
    filter_presets::get(catalog.connection(), id)?.ok_or(PresetError::NotFound)
}

/// Nom nettoyé (espaces autour retirés, forme Unicode composée), libre
/// parmi les préréglages autres que `except`.
fn free_name(
    catalog: &Catalog,
    name: &str,
    except: Option<FilterPresetId>,
) -> Result<String, PresetError> {
    let name: String = name.trim().nfc().collect();
    if name.is_empty() {
        return Err(PresetError::EmptyName);
    }
    if name.chars().count() > MAX_KEYWORD_CHARS {
        return Err(PresetError::TooLong);
    }
    if filter_presets::find_by_name(catalog.connection(), &name)?
        .is_some_and(|other| Some(other) != except)
    {
        return Err(PresetError::NameTaken(name));
    }
    Ok(name)
}
