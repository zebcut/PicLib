//! Panneau « Mots-clés » : mots-clés de la sélection, saisie
//! avec autocomplétion, ajout et retrait annulables, suggestions.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use crate::catalog::Catalog;
use crate::catalog::repositories::keywords;
use crate::domain::{KeywordEntryError, KeywordId, MediaId};
use crate::edits::{Edit, History};
use crate::keywords::{
    KeywordError, KeywordTree, Presence, SelectionKeyword, Suggestion, resolve_entry,
    selection_keywords, suggested_keywords, suggestions,
};

/// Longueur maximale d'une saisie, en caractères : bien au-delà d'une
/// liste ordinaire, mais évite d'enregistrer par erreur un texte énorme.
const MAX_ENTRY_CHARS: usize = 10_000;

/// Présence d'un mot-clé saisi sur les photos sélectionnées.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KeywordPresenceDto {
    /// Sur aucune photo (seulement parent d'un mot-clé saisi).
    Absent,
    /// Sur une partie des photos.
    Partial,
    /// Sur toutes les photos.
    All,
}

/// Un mot-clé affiché pour la sélection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionKeywordDto {
    /// Identifiant du mot-clé.
    pub id: u32,
    /// Son parent (`null` : à la racine).
    pub parent_id: Option<u32>,
    /// Nom.
    pub name: String,
    /// Noms de ses parents, du plus haut au plus proche.
    pub parents: Vec<String>,
    /// Vrai s'il concerne toutes les photos sélectionnées, lui-même ou par
    /// un descendant.
    pub on_all: bool,
    /// Présence du mot-clé saisi lui-même (s'il est saisi, il peut être
    /// retiré).
    pub entered: KeywordPresenceDto,
}

/// Mots-clés de la sélection : de quoi afficher l'arbre, et ce qui sera
/// exporté.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordPanel {
    /// Mots-clés saisis et leurs parents.
    pub with_parents: Vec<SelectionKeywordDto>,
    /// Mots-clés qui seront exportés.
    pub exported: Vec<SelectionKeywordDto>,
}

/// Suggestion de l'autocomplétion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordSuggestionDto {
    /// Identifiant du mot-clé.
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Noms de ses parents, du plus haut au plus proche.
    pub parents: Vec<String>,
    /// Vrai si un autre mot-clé porte le même nom (le chemin est alors
    /// affiché).
    pub ambiguous: bool,
}

/// Raison pour laquelle une saisie n'a pas été appliquée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Ce nom seul désigne plusieurs mots-clés : choisir une suggestion ou
    /// taper le chemin.
    Ambiguous {
        /// Nom concerné.
        name: String,
    },
    /// Nom de plus de 200 caractères, ou saisie trop longue.
    TooLong,
    /// Nom contenant « | ».
    ForbiddenCharacter {
        /// Nom concerné.
        name: String,
    },
}

/// Mots-clés des médias sélectionnés, avec leurs parents.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_keyword_panel(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<KeywordPanel, EditFailure> {
    let ids = media_ids_of(media_ids);
    let catalog = lock(&catalog);
    let connection = catalog.connection();
    let tree = KeywordTree::new(keywords::all(connection).map_err(|error| failure(&error))?);
    let existing = keywords::existing_media(connection, &ids).map_err(|error| failure(&error))?;
    let links = keywords::links(connection, &existing).map_err(|error| failure(&error))?;
    let lists = selection_keywords(&tree, &links, existing.len());
    let dto =
        |list: Vec<SelectionKeyword>| list.into_iter().map(SelectionKeywordDto::from).collect();
    Ok(KeywordPanel {
        with_parents: dto(lists.with_parents),
        exported: dto(lists.exported),
    })
}

/// Ajoute aux médias indiqués les mots-clés d'une saisie comme `Lieux >
/// France > Paris, Vacances` (niveaux manquants créés), de façon annulable.
/// Les mots-clés créés restent dans la liste après une annulation.
///
/// # Errors
///
/// Voir [`KeywordFailure`] ; rien n'est modifié ni créé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn add_keywords(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    entry: String,
) -> Result<EditOutcome, KeywordFailure> {
    if entry.chars().count() > MAX_ENTRY_CHARS {
        return Err(KeywordFailure::TooLong);
    }
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    // Les mots-clés sont créés dans une première transaction : une saisie
    // refusée (nom ambigu…) n'en laisse aucun.
    let transaction = catalog
        .transaction()
        .map_err(|error| catalog_failure(&error))?;
    let added = resolve_entry(&transaction, &entry).map_err(keyword_failure)?;
    transaction
        .commit()
        .map_err(|error| catalog_failure(&error.into()))?;
    let summary = Edit::add_keywords(&catalog, &added, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| catalog_failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Retire un mot-clé des médias indiqués, de façon annulable.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn remove_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    keyword_id: u32,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = Edit::remove_keyword(&catalog, KeywordId::new(i64::from(keyword_id)), &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Suggestions pour le mot-clé en cours de saisie (ce qui suit la dernière
/// virgule, par exemple `Lieux > Pays-Bas > Ze`).
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn suggest_keywords(
    catalog: State<'_, Mutex<Catalog>>,
    token: String,
) -> Result<Vec<KeywordSuggestionDto>, EditFailure> {
    let catalog = lock(&catalog);
    let found = suggestions(catalog.connection(), &token).map_err(|error| failure(&error))?;
    Ok(found.into_iter().map(KeywordSuggestionDto::from).collect())
}

/// Mots-clés suggérés pour la photo active `media_id` (sans photo active :
/// les mots-clés récents), neuf au plus, du plus probable au moins probable.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_suggested_keywords(
    catalog: State<'_, Mutex<Catalog>>,
    media_id: Option<u32>,
) -> Result<Vec<u32>, EditFailure> {
    let catalog = lock(&catalog);
    let found = suggested_keywords(
        catalog.connection(),
        media_id.map(|id| MediaId::new(i64::from(id))),
    )
    .map_err(|error| failure(&error))?;
    Ok(found.into_iter().map(keyword_id_dto).collect())
}

fn catalog_failure(error: &crate::catalog::CatalogError) -> KeywordFailure {
    failure(error);
    KeywordFailure::CatalogUnavailable
}

fn keyword_failure(error: KeywordError) -> KeywordFailure {
    match error {
        KeywordError::Ambiguous(name) => KeywordFailure::Ambiguous { name },
        KeywordError::Entry(KeywordEntryError::TooLong(_)) => KeywordFailure::TooLong,
        KeywordError::Entry(KeywordEntryError::ForbiddenCharacter(name)) => {
            KeywordFailure::ForbiddenCharacter { name }
        }
        KeywordError::Catalog(error) => catalog_failure(&error),
    }
}

/// Identifiant transmis à l'interface (un catalogue n'atteint jamais 4
/// milliards de mots-clés).
pub(super) fn keyword_id_dto(id: KeywordId) -> u32 {
    u32::try_from(id.get()).unwrap_or(u32::MAX)
}

impl From<SelectionKeyword> for SelectionKeywordDto {
    fn from(keyword: SelectionKeyword) -> Self {
        Self {
            id: keyword_id_dto(keyword.id),
            parent_id: keyword.parent_id.map(keyword_id_dto),
            name: keyword.name,
            parents: keyword.parents,
            on_all: keyword.on_all,
            entered: match keyword.entered {
                Presence::Absent => KeywordPresenceDto::Absent,
                Presence::Partial => KeywordPresenceDto::Partial,
                Presence::All => KeywordPresenceDto::All,
            },
        }
    }
}

impl From<Suggestion> for KeywordSuggestionDto {
    fn from(suggestion: Suggestion) -> Self {
        Self {
            id: keyword_id_dto(suggestion.id),
            name: suggestion.name,
            parents: suggestion.parents,
            ambiguous: suggestion.ambiguous,
        }
    }
}
