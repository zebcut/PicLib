//! Jeux de mots-clés : neuf mots-clés à poser en un clic ou
//! avec Option+1 à Option+9.
//!
//! Un jeu est un réglage, pas une donnée des photos : ses modifications se
//! font dans une fenêtre qu'on valide ou qu'on annule, hors de l'historique
//! (Cmd+Z). Poser ou retirer un mot-clé du jeu, en revanche, passe par
//! l'historique comme toute saisie.

use std::collections::HashSet;

use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use super::{KeywordError, resolve_entry};
use crate::catalog::CatalogError;
use crate::catalog::repositories::keyword_sets::{self, KeywordSet, SLOT_COUNT};
use crate::catalog::repositories::keywords;
use crate::domain::{KeywordEntryError, KeywordId, KeywordSetId, MAX_KEYWORD_CHARS};

/// Opération refusée sur les jeux de mots-clés.
#[derive(Debug, thiserror::Error)]
pub enum SetError {
    /// Nom vide.
    #[error("empty keyword set name")]
    EmptyName,
    /// Nom trop long.
    #[error("keyword set name too long")]
    TooLong,
    /// Un autre jeu porte déjà ce nom.
    #[error("keyword set name {0:?} already used")]
    NameTaken(String),
    /// Le jeu n'existe pas (ou plus).
    #[error("keyword set not found")]
    NotFound,
    /// Le texte d'une case ne désigne pas un seul mot-clé.
    #[error("slot {slot}: {error}")]
    Slot {
        /// Numéro de la case, de 1 à 9.
        slot: u8,
        /// Raison du refus.
        error: SlotError,
    },
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Raison pour laquelle le texte d'une case est refusé.
#[derive(Debug, thiserror::Error)]
pub enum SlotError {
    /// Plusieurs mots-clés séparés par des virgules : une case n'en tient
    /// qu'un.
    #[error("several keywords in one slot")]
    Several,
    /// Nom trop long ou caractère interdit.
    #[error(transparent)]
    Entry(#[from] KeywordEntryError),
    /// Un nom seul désigne plusieurs mots-clés, aucun à la racine.
    #[error("keyword {0:?} exists in several places")]
    Ambiguous(String),
}

/// Contenu d'une case, tel que rempli dans la fenêtre du jeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotInput {
    /// Mot-clé choisi dans les suggestions, s'il l'a été : il désigne
    /// exactement ce mot-clé, même si un autre porte le même nom.
    pub keyword: Option<KeywordId>,
    /// Texte de la case : un chemin (`Lieux > Paris`) ou un nom seul, lu
    /// comme à la saisie (et créé s'il n'existe pas) quand aucun mot-clé
    /// n'a été choisi. Vide : case vide.
    pub text: String,
}

/// Tous les jeux, dans l'ordre de leur nom.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn keyword_sets(connection: &Connection) -> Result<Vec<KeywordSet>, CatalogError> {
    keyword_sets::all(connection)
}

/// Crée un jeu vide, qui devient le jeu actif.
///
/// À appeler dans une transaction.
///
/// # Errors
///
/// Voir [`SetError`].
pub fn create(connection: &Connection, name: &str) -> Result<KeywordSetId, SetError> {
    let name = parse_set_name(name)?;
    if keyword_sets::find_by_name(connection, &name)?.is_some() {
        return Err(SetError::NameTaken(name));
    }
    let id = keyword_sets::create(connection, &name)?;
    keyword_sets::set_active(connection, Some(id))?;
    Ok(id)
}

/// Renomme un jeu ; changer seulement les majuscules est permis.
///
/// # Errors
///
/// Voir [`SetError`].
pub fn rename(connection: &Connection, id: KeywordSetId, name: &str) -> Result<(), SetError> {
    let name = parse_set_name(name)?;
    ensure_exists(connection, id)?;
    if keyword_sets::find_by_name(connection, &name)?.is_some_and(|other| other != id) {
        return Err(SetError::NameTaken(name));
    }
    Ok(keyword_sets::rename(connection, id, &name)?)
}

/// Supprime un jeu (ses mots-clés restent). S'il était actif, le premier
/// des jeux restants le devient.
///
/// À appeler dans une transaction.
///
/// # Errors
///
/// Voir [`SetError`].
pub fn delete(connection: &Connection, id: KeywordSetId) -> Result<(), SetError> {
    let sets = keyword_sets::all(connection)?;
    let set = sets
        .iter()
        .find(|set| set.id == id)
        .ok_or(SetError::NotFound)?;
    keyword_sets::delete(connection, id)?;
    if set.active {
        let next = sets
            .iter()
            .find(|other| other.id != id)
            .map(|other| other.id);
        keyword_sets::set_active(connection, next)?;
    }
    Ok(())
}

/// Rend un jeu actif.
///
/// À appeler dans une transaction.
///
/// # Errors
///
/// Voir [`SetError`].
pub fn activate(connection: &Connection, id: KeywordSetId) -> Result<(), SetError> {
    ensure_exists(connection, id)?;
    Ok(keyword_sets::set_active(connection, Some(id))?)
}

/// Remplit les neuf cases d'un jeu. Une case désigne le mot-clé choisi
/// dans les suggestions s'il existe encore, sinon celui que désigne son
/// texte, créé s'il n'existe pas (comme à la saisie).
///
/// À appeler dans une transaction : si une case est refusée, rien ne doit
/// rester créé ni modifié.
///
/// # Errors
///
/// Voir [`SetError`].
pub fn fill(
    connection: &Connection,
    id: KeywordSetId,
    inputs: &[SlotInput; SLOT_COUNT],
) -> Result<(), SetError> {
    ensure_exists(connection, id)?;
    let existing: HashSet<KeywordId> = keywords::all(connection)?
        .into_iter()
        .map(|keyword| keyword.id)
        .collect();
    let mut slots = [None; SLOT_COUNT];
    for ((slot, input), cell) in (1_u8..).zip(inputs).zip(&mut slots) {
        *cell = slot_keyword(connection, &existing, input).map_err(|error| error.for_slot(slot))?;
    }
    Ok(keyword_sets::replace_slots(connection, id, &slots)?)
}

/// Mot-clé d'une case, ou `None` si elle est vide.
fn slot_keyword(
    connection: &Connection,
    existing: &HashSet<KeywordId>,
    input: &SlotInput,
) -> Result<Option<KeywordId>, FillError> {
    if input.text.trim().is_empty() {
        return Ok(None);
    }
    if let Some(keyword) = input.keyword.filter(|keyword| existing.contains(keyword)) {
        return Ok(Some(keyword));
    }
    match resolve_entry(connection, &input.text)?.as_slice() {
        [] => Ok(None),
        [keyword] => Ok(Some(*keyword)),
        _ => Err(FillError::Slot(SlotError::Several)),
    }
}

/// Erreur en remplissant une case, avant de savoir laquelle.
enum FillError {
    Slot(SlotError),
    Catalog(CatalogError),
}

impl FillError {
    fn for_slot(self, slot: u8) -> SetError {
        match self {
            Self::Slot(error) => SetError::Slot { slot, error },
            Self::Catalog(error) => SetError::Catalog(error),
        }
    }
}

impl From<KeywordError> for FillError {
    fn from(error: KeywordError) -> Self {
        match error {
            KeywordError::Entry(error) => Self::Slot(SlotError::Entry(error)),
            KeywordError::Ambiguous(name) => Self::Slot(SlotError::Ambiguous(name)),
            KeywordError::Catalog(error) => Self::Catalog(error),
        }
    }
}

fn ensure_exists(connection: &Connection, id: KeywordSetId) -> Result<(), SetError> {
    if keyword_sets::exists(connection, id)? {
        Ok(())
    } else {
        Err(SetError::NotFound)
    }
}

/// Nom d'un jeu : espaces autour ignorés, forme Unicode composée.
fn parse_set_name(name: &str) -> Result<String, SetError> {
    let name: String = name.trim().nfc().collect();
    if name.is_empty() {
        Err(SetError::EmptyName)
    } else if name.chars().count() > MAX_KEYWORD_CHARS {
        Err(SetError::TooLong)
    } else {
        Ok(name)
    }
}
