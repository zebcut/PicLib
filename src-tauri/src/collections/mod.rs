//! Collections manuelles, dossiers de collections, collection rapide et
//! collection cible(10.3 et 10.4) :
//! chaque changement est préparé ici, vérifié, puis fait de façon annulable
//! (Cmd+Z). Changer de cible ne s'annule pas : c'est un réglage.

use unicode_normalization::UnicodeNormalization;

use crate::catalog::repositories::collections::{self, CollectionKind, CollectionNode};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{CollectionId, MAX_KEYWORD_CHARS, MediaId};
use crate::edits::{CollectionChange, CollectionEdit, CollectionStep, Edit, EditSummary, History};

mod quick;

pub use quick::{clear_quick, save_quick, set_target, toggle_in_target};

/// Changement refusé.
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    /// Nom vide.
    #[error("empty collection name")]
    EmptyName,
    /// Nom trop long.
    #[error("collection name too long")]
    TooLong,
    /// Un voisin porte déjà ce nom.
    #[error("collection name {0:?} already used")]
    NameTaken(String),
    /// La collection ou le dossier n'existe pas (ou plus).
    #[error("collection not found")]
    NotFound,
    /// Only a collection set or a smart collection (for smart collections)
    /// holds children, and only a manual collection holds photos.
    #[error("wrong kind of collection")]
    WrongKind,
    /// A set or a smart collection cannot go inside its own branch.
    #[error("cannot move a folder into itself")]
    IntoItself,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Ce qu'une suppression emporterait, pour la confirmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletionSummary {
    /// Collections dans la branche (le nœud compris s'il en est une).
    pub collections: usize,
    /// Dossiers de collections dans la branche (le nœud compris s'il en est
    /// un).
    pub folders: usize,
}

/// Crée une collection ou un dossier sous `parent` ; une collection reçoit
/// aussitôt `media` (les photos sélectionnées), dans la même modification.
///
/// # Errors
///
/// Voir [`CollectionError`] ; rien n'est créé en cas d'échec.
pub fn create(
    catalog: &mut Catalog,
    history: &mut History,
    parent: Option<CollectionId>,
    kind: CollectionKind,
    name: &str,
    media: &[MediaId],
) -> Result<(CollectionId, EditSummary), CollectionError> {
    let connection = catalog.connection();
    if let Some(parent) = parent {
        folder(connection, parent)?;
    }
    let name = free_name(connection, parent, name, None)?;
    let node = CollectionNode {
        id: collections::next_id(connection)?,
        parent_id: parent,
        kind,
        name,
        rules: None,
    };
    let id = node.id;
    let mut steps = vec![CollectionStep::Create(node)];
    if kind == CollectionKind::Collection && !media.is_empty() {
        steps.push(CollectionStep::Add {
            collection: id,
            media: distinct(collections::existing_media(connection, media)?),
        });
    }
    let summary = perform(catalog, history, CollectionChange::Created, steps)?;
    Ok((id, summary))
}

/// Crée une collection dynamique sous `parent`, avec ses règles (JSON,
/// vérifiées par l'appelant).
///
/// # Errors
///
/// Voir [`CollectionError`] ; rien n'est créé en cas d'échec.
pub fn create_smart(
    catalog: &mut Catalog,
    history: &mut History,
    parent: Option<CollectionId>,
    name: &str,
    rules: String,
) -> Result<(CollectionId, EditSummary), CollectionError> {
    let connection = catalog.connection();
    if let Some(parent) = parent {
        smart_parent(connection, parent)?;
    }
    let name = free_name(connection, parent, name, None)?;
    let node = CollectionNode {
        id: collections::next_id(connection)?,
        parent_id: parent,
        kind: CollectionKind::Collection,
        name,
        rules: Some(rules),
    };
    let id = node.id;
    let summary = perform(
        catalog,
        history,
        CollectionChange::Created,
        vec![CollectionStep::Create(node)],
    )?;
    Ok((id, summary))
}

/// Remplace les règles d'une collection dynamique.
///
/// # Errors
///
/// Voir [`CollectionError`] : une collection manuelle n'a pas de règles.
pub fn set_rules(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
    rules: String,
) -> Result<EditSummary, CollectionError> {
    let node = existing(catalog.connection(), id)?;
    if !node.is_smart() {
        return Err(CollectionError::WrongKind);
    }
    let steps = vec![CollectionStep::SetRules {
        id,
        before: node.rules,
        after: Some(rules),
    }];
    perform(catalog, history, CollectionChange::RulesChanged, steps)
}

/// Renomme.
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn rename(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
    name: &str,
) -> Result<EditSummary, CollectionError> {
    let node = existing(catalog.connection(), id)?;
    let name = free_name(catalog.connection(), node.parent_id, name, Some(id))?;
    let steps = vec![CollectionStep::Rename {
        id,
        before: node.name,
        after: name,
    }];
    perform(catalog, history, CollectionChange::Renamed, steps)
}

/// Range sous un autre dossier (`None` : à la racine).
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn move_to(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
    parent: Option<CollectionId>,
) -> Result<EditSummary, CollectionError> {
    let connection = catalog.connection();
    let node = existing(connection, id)?;
    if let Some(parent) = parent {
        if node.is_smart() {
            smart_parent(connection, parent)?;
        } else {
            folder(connection, parent)?;
        }
        if collections::branch(connection, id)?
            .iter()
            .any(|inside| inside.id == parent)
        {
            return Err(CollectionError::IntoItself);
        }
    }
    free_name(connection, parent, &node.name, Some(id))?;
    let steps = vec![CollectionStep::Reparent {
        id,
        before: node.parent_id,
        after: parent,
    }];
    perform(catalog, history, CollectionChange::Moved, steps)
}

/// Ce qu'emporterait la suppression de `id`.
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn deletion_summary(
    catalog: &Catalog,
    id: CollectionId,
) -> Result<DeletionSummary, CollectionError> {
    existing(catalog.connection(), id)?;
    let branch = collections::branch(catalog.connection(), id)?;
    let folders = branch
        .iter()
        .filter(|node| node.kind == CollectionKind::Folder)
        .count();
    Ok(DeletionSummary {
        collections: branch.len() - folders,
        folders,
    })
}

/// Supprime une collection, ou un dossier avec tout ce qu'il contient ; les
/// photos restent au catalogue.
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn delete(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
) -> Result<EditSummary, CollectionError> {
    let connection = catalog.connection();
    existing(connection, id)?;
    let branch = collections::branch(connection, id)?;
    let mut steps = Vec::new();
    for node in &branch {
        if node.kind == CollectionKind::Collection {
            steps.push(CollectionStep::Remove {
                collection: node.id,
                media: collections::media_of(connection, node.id)?,
            });
        }
    }
    // Les enfants avant leur parent.
    steps.extend(branch.into_iter().rev().map(CollectionStep::Delete));
    perform(catalog, history, CollectionChange::Deleted, steps)
}

/// Met des photos dans une collection (celles qui y sont déjà sont
/// ignorées).
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn add_media(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
    media: &[MediaId],
) -> Result<EditSummary, CollectionError> {
    let connection = catalog.connection();
    collection(connection, id)?;
    let present: std::collections::HashSet<MediaId> = collections::present(connection, id, media)?
        .into_iter()
        .collect();
    let added = distinct(collections::existing_media(connection, media)?)
        .into_iter()
        .filter(|media| !present.contains(media))
        .collect();
    let steps = vec![CollectionStep::Add {
        collection: id,
        media: added,
    }];
    perform(catalog, history, CollectionChange::MediaAdded, steps)
}

/// Retire des photos d'une collection ; elles restent au catalogue.
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn remove_media(
    catalog: &mut Catalog,
    history: &mut History,
    id: CollectionId,
    media: &[MediaId],
) -> Result<EditSummary, CollectionError> {
    let connection = catalog.connection();
    collection(connection, id)?;
    let steps = vec![CollectionStep::Remove {
        collection: id,
        media: distinct(collections::present(connection, id, media)?),
    }];
    perform(catalog, history, CollectionChange::MediaRemoved, steps)
}

fn perform(
    catalog: &mut Catalog,
    history: &mut History,
    change: CollectionChange,
    steps: Vec<CollectionStep>,
) -> Result<EditSummary, CollectionError> {
    let edit = Edit::Collection(CollectionEdit::new(change, steps));
    Ok(history.perform(catalog, edit)?)
}

/// Chaque photo une seule fois, dans l'ordre.
fn distinct(media: Vec<MediaId>) -> Vec<MediaId> {
    let mut seen = std::collections::HashSet::new();
    media.into_iter().filter(|id| seen.insert(*id)).collect()
}

fn existing(
    connection: &rusqlite::Connection,
    id: CollectionId,
) -> Result<CollectionNode, CollectionError> {
    collections::get(connection, id)?.ok_or(CollectionError::NotFound)
}

fn folder(connection: &rusqlite::Connection, id: CollectionId) -> Result<(), CollectionError> {
    match existing(connection, id)?.kind {
        CollectionKind::Folder => Ok(()),
        CollectionKind::Collection => Err(CollectionError::WrongKind),
    }
}

/// A parent for a smart collection: a collection set, or another smart
/// collection that it then refines (its rules apply on top of the parent's).
/// Manual collections never hold children.
fn smart_parent(
    connection: &rusqlite::Connection,
    id: CollectionId,
) -> Result<(), CollectionError> {
    let node = existing(connection, id)?;
    match node.kind {
        CollectionKind::Folder => Ok(()),
        CollectionKind::Collection if node.is_smart() => Ok(()),
        CollectionKind::Collection => Err(CollectionError::WrongKind),
    }
}

/// Une collection manuelle : un dossier n'a pas de photos, et celles d'une
/// collection dynamique sont calculées.
fn collection(connection: &rusqlite::Connection, id: CollectionId) -> Result<(), CollectionError> {
    let node = existing(connection, id)?;
    match node.kind {
        CollectionKind::Collection if !node.is_smart() => Ok(()),
        _ => Err(CollectionError::WrongKind),
    }
}

/// Nom nettoyé (espaces autour retirés, forme Unicode composée), libre
/// parmi les voisins de `parent` autres que `except`.
fn free_name(
    connection: &rusqlite::Connection,
    parent: Option<CollectionId>,
    name: &str,
    except: Option<CollectionId>,
) -> Result<String, CollectionError> {
    let name: String = name.trim().nfc().collect();
    if name.is_empty() {
        return Err(CollectionError::EmptyName);
    }
    if name.chars().count() > MAX_KEYWORD_CHARS {
        return Err(CollectionError::TooLong);
    }
    if collections::find_sibling(connection, parent, &name)?
        .is_some_and(|other| Some(other) != except)
    {
        return Err(CollectionError::NameTaken(name));
    }
    Ok(name)
}
