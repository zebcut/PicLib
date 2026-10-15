//! Jeux de mots-clés : lire les jeux, en créer, renommer,
//! supprimer, choisir le jeu actif, remplir ses neuf cases. Ces réglages
//! ne passent pas par l'historique (Cmd+Z) ; poser ou retirer un mot-clé
//! du jeu, si : ce sont les commandes `add_keyword` et `remove_keyword`.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::edits::{failure, lock};
use super::keywords::keyword_id_dto;
use crate::catalog::repositories::keyword_sets::{KeywordSet, SLOT_COUNT};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{KeywordEntryError, KeywordId, KeywordSetId};
use crate::keywords::sets::{self, SetError, SlotError, SlotInput};

/// Un jeu de mots-clés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordSetDto {
    /// Identifiant.
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Vrai pour le jeu affiché et servi par Option+1 à Option+9.
    pub active: bool,
    /// Mot-clé de chacune des neuf cases (`null` : case vide).
    pub slots: Vec<Option<u32>>,
}

/// Contenu d'une case, tel que rempli dans la fenêtre du jeu.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordSetSlotInput {
    /// Mot-clé choisi dans les suggestions, s'il l'a été.
    pub keyword_id: Option<u32>,
    /// Texte de la case (vide : case vide).
    pub text: String,
}

/// Opération refusée sur les jeux de mots-clés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordSetFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Le jeu n'existe plus.
    NotFound,
    /// Nom vide.
    EmptyName,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Un autre jeu porte déjà ce nom.
    NameTaken {
        /// Nom concerné.
        name: String,
    },
    /// Le texte d'une case ne désigne pas un seul mot-clé.
    Slot {
        /// Numéro de la case, de 1 à 9.
        slot: u8,
        /// Raison du refus.
        reason: KeywordSetSlotRefusal,
    },
    /// L'interface n'a pas envoyé neuf cases.
    InvalidSlots,
}

/// Raison pour laquelle le texte d'une case est refusé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordSetSlotRefusal {
    /// Plusieurs mots-clés séparés par des virgules.
    Several,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Nom contenant « | ».
    ForbiddenCharacter {
        /// Nom concerné.
        name: String,
    },
    /// Un nom seul désigne plusieurs mots-clés, aucun à la racine.
    Ambiguous {
        /// Nom concerné.
        name: String,
    },
}

/// Tous les jeux de mots-clés, dans l'ordre de leur nom.
///
/// # Errors
///
/// [`KeywordSetFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_keyword_sets(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<KeywordSetDto>, KeywordSetFailure> {
    let catalog = lock(&catalog);
    let sets = sets::keyword_sets(catalog.connection()).map_err(|error| catalog_failure(&error))?;
    Ok(sets.into_iter().map(KeywordSetDto::from).collect())
}

/// Crée un jeu vide, qui devient le jeu actif, et renvoie son identifiant.
///
/// # Errors
///
/// Voir [`KeywordSetFailure`] ; rien n'est créé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn create_keyword_set(
    catalog: State<'_, Mutex<Catalog>>,
    name: String,
) -> Result<u32, KeywordSetFailure> {
    let id = write(&catalog, |connection| sets::create(connection, &name))?;
    Ok(set_id_dto(id))
}

/// Renomme un jeu.
///
/// # Errors
///
/// Voir [`KeywordSetFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn rename_keyword_set(
    catalog: State<'_, Mutex<Catalog>>,
    set_id: u32,
    name: String,
) -> Result<(), KeywordSetFailure> {
    write(&catalog, |connection| {
        sets::rename(connection, set_id_of(set_id), &name)
    })
}

/// Supprime un jeu ; ses mots-clés restent.
///
/// # Errors
///
/// Voir [`KeywordSetFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn delete_keyword_set(
    catalog: State<'_, Mutex<Catalog>>,
    set_id: u32,
) -> Result<(), KeywordSetFailure> {
    write(&catalog, |connection| {
        sets::delete(connection, set_id_of(set_id))
    })
}

/// Rend un jeu actif.
///
/// # Errors
///
/// Voir [`KeywordSetFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn activate_keyword_set(
    catalog: State<'_, Mutex<Catalog>>,
    set_id: u32,
) -> Result<(), KeywordSetFailure> {
    write(&catalog, |connection| {
        sets::activate(connection, set_id_of(set_id))
    })
}

/// Remplit les neuf cases d'un jeu ; un mot-clé tapé qui n'existe pas est
/// créé, comme à la saisie.
///
/// # Errors
///
/// Voir [`KeywordSetFailure`] ; rien n'est créé ni modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn fill_keyword_set(
    catalog: State<'_, Mutex<Catalog>>,
    set_id: u32,
    slots: Vec<KeywordSetSlotInput>,
) -> Result<(), KeywordSetFailure> {
    let inputs: [SlotInput; SLOT_COUNT] = slots
        .into_iter()
        .map(|slot| SlotInput {
            keyword: slot.keyword_id.map(|id| KeywordId::new(i64::from(id))),
            text: slot.text,
        })
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| KeywordSetFailure::InvalidSlots)?;
    write(&catalog, |connection| {
        sets::fill(connection, set_id_of(set_id), &inputs)
    })
}

/// Fait `change` dans une transaction, validée seulement s'il réussit.
fn write<T>(
    catalog: &Mutex<Catalog>,
    change: impl FnOnce(&rusqlite::Connection) -> Result<T, SetError>,
) -> Result<T, KeywordSetFailure> {
    let mut catalog = lock(catalog);
    let transaction = catalog
        .transaction()
        .map_err(|error| catalog_failure(&error))?;
    let value = change(&transaction).map_err(set_failure)?;
    transaction
        .commit()
        .map_err(|error| catalog_failure(&error.into()))?;
    Ok(value)
}

fn set_id_of(id: u32) -> KeywordSetId {
    KeywordSetId::new(i64::from(id))
}

fn set_id_dto(id: KeywordSetId) -> u32 {
    u32::try_from(id.get()).unwrap_or(u32::MAX)
}

fn catalog_failure(error: &CatalogError) -> KeywordSetFailure {
    failure(error);
    KeywordSetFailure::CatalogUnavailable
}

fn set_failure(error: SetError) -> KeywordSetFailure {
    match error {
        SetError::EmptyName => KeywordSetFailure::EmptyName,
        SetError::TooLong => KeywordSetFailure::TooLong,
        SetError::NameTaken(name) => KeywordSetFailure::NameTaken { name },
        SetError::NotFound => KeywordSetFailure::NotFound,
        SetError::Slot { slot, error } => KeywordSetFailure::Slot {
            slot,
            reason: match error {
                SlotError::Several => KeywordSetSlotRefusal::Several,
                SlotError::Entry(KeywordEntryError::TooLong(_)) => KeywordSetSlotRefusal::TooLong,
                SlotError::Entry(KeywordEntryError::ForbiddenCharacter(name)) => {
                    KeywordSetSlotRefusal::ForbiddenCharacter { name }
                }
                SlotError::Ambiguous(name) => KeywordSetSlotRefusal::Ambiguous { name },
            },
        },
        SetError::Catalog(error) => catalog_failure(&error),
    }
}

impl From<KeywordSet> for KeywordSetDto {
    fn from(set: KeywordSet) -> Self {
        Self {
            id: set_id_dto(set.id),
            name: set.name,
            active: set.active,
            slots: set
                .slots
                .iter()
                .map(|slot| slot.map(keyword_id_dto))
                .collect(),
        }
    }
}
