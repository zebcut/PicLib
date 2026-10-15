//! Panneau « Liste des mots-clés » : l'arbre avec le nombre de
//! photos de chaque mot-clé, les cases à cocher de la sélection, la
//! création, le renommage, le déplacement, la fusion, la suppression et
//! « inclure à l'export ». Tout est annulable, sauf la création.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::State;

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::keywords::keyword_id_dto;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{KeywordEntryError, KeywordId};
use crate::edits::{Edit, History};
use crate::keywords::manage::{self, KeywordListItem, ManageError};

/// Un mot-clé de la liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordListItemDto {
    /// Identifiant.
    pub id: u32,
    /// Parent (`null` : à la racine).
    pub parent_id: Option<u32>,
    /// Nom.
    pub name: String,
    /// « Inclure à l'export ».
    pub include_on_export: bool,
    /// Photos qui portent ce mot-clé ou un de ses descendants.
    pub photo_count: u32,
}

/// Opération refusée sur la liste des mots-clés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordListFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Le mot-clé n'existe plus.
    NotFound,
    /// Nom vide.
    EmptyName,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Nom contenant « , », « > » ou « | ».
    ForbiddenCharacter {
        /// Nom concerné.
        name: String,
    },
    /// Un autre mot-clé porte déjà ce nom au même niveau.
    NameTaken {
        /// Nom concerné.
        name: String,
    },
    /// Fusion d'un mot-clé avec lui-même ou avec un de ses descendants.
    MergeIntoBranch,
    /// Déplacement d'un mot-clé sous lui-même ou sous un de ses
    /// descendants.
    MoveIntoBranch,
}

/// Tous les mots-clés, avec le nombre de photos de chacun.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_keyword_list(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<KeywordListItemDto>, EditFailure> {
    let catalog = lock(&catalog);
    let list = manage::keyword_list(catalog.connection()).map_err(|error| failure(&error))?;
    Ok(list.into_iter().map(KeywordListItemDto::from).collect())
}

/// Pose un mot-clé sur les médias indiqués (case cochée), de façon
/// annulable.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn add_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    keyword_id: u32,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = Edit::add_keywords(&catalog, &[keyword_id_of(keyword_id)], &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Crée un mot-clé sous `parent_id` (`null` : à la racine) et renvoie son
/// identifiant. Pas d'annulation : il reste dans la liste, comme à la
/// saisie.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est créé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn create_keyword(
    catalog: State<'_, Mutex<Catalog>>,
    parent_id: Option<u32>,
    name: String,
    include_on_export: bool,
) -> Result<u32, KeywordListFailure> {
    let mut catalog = lock(&catalog);
    let transaction = catalog
        .transaction()
        .map_err(|error| catalog_failure(&error))?;
    let id = manage::create(
        &transaction,
        parent_id.map(keyword_id_of),
        &name,
        include_on_export,
    )
    .map_err(manage_failure)?;
    transaction
        .commit()
        .map_err(|error| catalog_failure(&error.into()))?;
    Ok(keyword_id_dto(id))
}

/// Renomme un mot-clé, de façon annulable.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn rename_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    keyword_id: u32,
    name: String,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        manage::rename(catalog.connection(), keyword_id_of(keyword_id), &name)
    })
}

/// Déplace un mot-clé, avec toute sa branche, sous `parent_id` (`null` : à
/// la racine), de façon annulable.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn move_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    keyword_id: u32,
    parent_id: Option<u32>,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        manage::move_keyword(
            catalog.connection(),
            keyword_id_of(keyword_id),
            parent_id.map(keyword_id_of),
        )
    })
}

/// Fusionne un mot-clé dans un autre, de façon annulable : ses photos et
/// ses sous-mots-clés passent sur `into_id`, puis il disparaît.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn merge_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    keyword_id: u32,
    into_id: u32,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        manage::merge(
            catalog.connection(),
            keyword_id_of(keyword_id),
            keyword_id_of(into_id),
        )
    })
}

/// Supprime un mot-clé et ses descendants (retirés des photos), de façon
/// annulable.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn delete_keyword(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    keyword_id: u32,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        manage::delete(catalog.connection(), keyword_id_of(keyword_id))
    })
}

/// Supprime les mots-clés dont aucune photo ne porte ni le mot-clé ni un
/// descendant, de façon annulable.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn delete_unused_keywords(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        Ok(manage::delete_unused(catalog.connection())?)
    })
}

/// Coche ou décoche « inclure à l'export » d'un mot-clé, de façon
/// annulable.
///
/// # Errors
///
/// Voir [`KeywordListFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn set_keyword_export(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    keyword_id: u32,
    include: bool,
) -> Result<EditOutcome, KeywordListFailure> {
    perform(&history, &catalog, |catalog| {
        manage::set_include_on_export(catalog.connection(), keyword_id_of(keyword_id), include)
    })
}

/// Prépare une modification de l'arbre avec `prepare`, puis l'applique et
/// l'ajoute à l'historique.
fn perform(
    history: &Mutex<History>,
    catalog: &Mutex<Catalog>,
    prepare: impl FnOnce(&Catalog) -> Result<Edit, ManageError>,
) -> Result<EditOutcome, KeywordListFailure> {
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(history);
    let mut catalog = lock(catalog);
    let edit = prepare(&catalog).map_err(manage_failure)?;
    let summary = history
        .perform(&mut catalog, edit)
        .map_err(|error| catalog_failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

fn keyword_id_of(id: u32) -> KeywordId {
    KeywordId::new(i64::from(id))
}

fn catalog_failure(error: &CatalogError) -> KeywordListFailure {
    failure(error);
    KeywordListFailure::CatalogUnavailable
}

fn manage_failure(error: ManageError) -> KeywordListFailure {
    match error {
        ManageError::Name(KeywordEntryError::TooLong(_)) => KeywordListFailure::TooLong,
        ManageError::Name(KeywordEntryError::ForbiddenCharacter(name)) => {
            KeywordListFailure::ForbiddenCharacter { name }
        }
        ManageError::EmptyName => KeywordListFailure::EmptyName,
        ManageError::NotFound => KeywordListFailure::NotFound,
        ManageError::NameTaken(name) => KeywordListFailure::NameTaken { name },
        ManageError::MergeIntoBranch => KeywordListFailure::MergeIntoBranch,
        ManageError::MoveIntoBranch => KeywordListFailure::MoveIntoBranch,
        ManageError::Catalog(error) => catalog_failure(&error),
    }
}

impl From<KeywordListItem> for KeywordListItemDto {
    fn from(item: KeywordListItem) -> Self {
        Self {
            id: keyword_id_dto(item.keyword.id),
            parent_id: item.keyword.parent_id.map(keyword_id_dto),
            name: item.keyword.name,
            include_on_export: item.keyword.include_on_export,
            photo_count: u32::try_from(item.photo_count).unwrap_or(u32::MAX),
        }
    }
}
