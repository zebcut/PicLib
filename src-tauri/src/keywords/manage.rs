//! Gestion de la liste des mots-clés : créer, renommer,
//! déplacer, fusionner, supprimer, « inclure à l'export ».
//!
//! Sauf la création, chaque opération prépare une modification annulable
//! ([`Edit::KeywordTree`]) sans rien écrire : c'est l'historique qui
//! l'applique. Un mot-clé créé reste, comme à la saisie.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use super::merge_plan::MergePlan;
use super::{KeywordTree, branch_photo_counts};
use crate::catalog::CatalogError;
use crate::catalog::repositories::keyword_changes;
use crate::catalog::repositories::keyword_sets::{self, SlotRef};
use crate::catalog::repositories::keywords::{self, Keyword};
use crate::domain::{KeywordEntryError, KeywordId, MediaId, parse_keyword_name};
use crate::edits::{Edit, KeywordTreeChange, KeywordTreeEdit, KeywordTreeStep};

/// Opération refusée sur la liste des mots-clés.
#[derive(Debug, thiserror::Error)]
pub enum ManageError {
    /// Nom trop long ou contenant « , », « > » ou « | ».
    #[error(transparent)]
    Name(#[from] KeywordEntryError),
    /// Nom vide.
    #[error("empty keyword name")]
    EmptyName,
    /// Le mot-clé n'existe pas (ou plus).
    #[error("keyword not found")]
    NotFound,
    /// Un autre mot-clé porte déjà ce nom au même niveau.
    #[error("keyword name {0:?} already used at this level")]
    NameTaken(String),
    /// Fusion d'un mot-clé avec lui-même ou avec un de ses descendants.
    #[error("cannot merge a keyword into its own branch")]
    MergeIntoBranch,
    /// Déplacement d'un mot-clé sous lui-même ou sous un de ses
    /// descendants.
    #[error("cannot move a keyword into its own branch")]
    MoveIntoBranch,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Un mot-clé de la liste, avec le nombre de photos de sa branche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordListItem {
    /// Le mot-clé.
    pub keyword: Keyword,
    /// Photos qui portent ce mot-clé ou un de ses descendants, chacune
    /// comptée une fois.
    pub photo_count: u64,
}

/// Tous les mots-clés, avec le nombre de photos de chacun.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn keyword_list(connection: &Connection) -> Result<Vec<KeywordListItem>, CatalogError> {
    let tree = load(connection)?;
    let counts = branch_photo_counts(&tree, keywords::all_links(connection)?);
    Ok(tree
        .keywords()
        .map(|keyword| KeywordListItem {
            photo_count: counts.get(&keyword.id).copied().unwrap_or(0),
            keyword: keyword.clone(),
        })
        .collect())
}

/// Crée un mot-clé sous `parent` (`None` : à la racine). Pas d'annulation :
/// un mot-clé créé reste dans la liste, comme à la saisie.
///
/// À appeler dans une transaction.
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn create(
    connection: &Connection,
    parent: Option<KeywordId>,
    name: &str,
    include_on_export: bool,
) -> Result<KeywordId, ManageError> {
    let name = parse_keyword_name(name)?.ok_or(ManageError::EmptyName)?;
    let tree = load(connection)?;
    if parent.is_some_and(|parent| tree.get(parent).is_none()) {
        return Err(ManageError::NotFound);
    }
    if tree.child_named(parent, &name).is_some() {
        return Err(ManageError::NameTaken(name));
    }
    let id = keywords::create(connection, parent, &name)?;
    if !include_on_export {
        keyword_changes::set_include_on_export(connection, id, false)?;
    }
    Ok(id)
}

/// Prépare le renommage d'un mot-clé. Changer seulement les majuscules est
/// permis ; prendre le nom d'un autre mot-clé du même niveau ne l'est pas
/// (il faut alors les fusionner).
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn rename(connection: &Connection, id: KeywordId, name: &str) -> Result<Edit, ManageError> {
    let name = parse_keyword_name(name)?.ok_or(ManageError::EmptyName)?;
    let tree = load(connection)?;
    let keyword = tree.get(id).ok_or(ManageError::NotFound)?;
    if tree
        .child_named(keyword.parent_id, &name)
        .is_some_and(|other| other != id)
    {
        return Err(ManageError::NameTaken(name));
    }
    let steps = if keyword.name == name {
        Vec::new()
    } else {
        vec![KeywordTreeStep::Rename {
            id,
            before: keyword.name.clone(),
            after: name,
        }]
    };
    let media = branch_media(connection, &tree, id)?;
    Ok(tree_edit(KeywordTreeChange::Renamed, steps, media))
}

/// Prépare le déplacement d'un mot-clé, avec toute sa branche, sous
/// `parent` (`None` : à la racine). Refusé dans sa propre branche, ou si un
/// autre mot-clé porte déjà ce nom à l'arrivée (il faut alors les
/// fusionner).
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn move_keyword(
    connection: &Connection,
    id: KeywordId,
    parent: Option<KeywordId>,
) -> Result<Edit, ManageError> {
    let tree = load(connection)?;
    let keyword = tree.get(id).ok_or(ManageError::NotFound)?;
    if let Some(parent) = parent {
        if tree.get(parent).is_none() {
            return Err(ManageError::NotFound);
        }
        if tree.is_within(parent, id) {
            return Err(ManageError::MoveIntoBranch);
        }
    }
    if keyword.parent_id == parent {
        return Ok(tree_edit(KeywordTreeChange::Moved, Vec::new(), Vec::new()));
    }
    if tree
        .child_named(parent, &keyword.name)
        .is_some_and(|other| other != id)
    {
        return Err(ManageError::NameTaken(keyword.name.clone()));
    }
    let steps = vec![KeywordTreeStep::Reparent {
        id,
        before: keyword.parent_id,
        after: parent,
    }];
    let media = branch_media(connection, &tree, id)?;
    Ok(tree_edit(KeywordTreeChange::Moved, steps, media))
}

/// Prépare le changement de « inclure à l'export » d'un mot-clé.
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn set_include_on_export(
    connection: &Connection,
    id: KeywordId,
    include: bool,
) -> Result<Edit, ManageError> {
    let tree = load(connection)?;
    let keyword = tree.get(id).ok_or(ManageError::NotFound)?;
    let steps = if keyword.include_on_export == include {
        Vec::new()
    } else {
        vec![KeywordTreeStep::Export { id, include }]
    };
    let media = branch_media(connection, &tree, id)?;
    Ok(tree_edit(KeywordTreeChange::Export(include), steps, media))
}

/// Prépare la fusion de `from` dans `into` : les photos de `from` passent
/// sur `into`, ses sous-mots-clés sont rangés sous `into` (fusionnés avec
/// ceux de même nom qui s'y trouvent déjà), les cases des jeux qui le
/// désignaient désignent `into`, puis `from` disparaît.
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn merge(
    connection: &Connection,
    from: KeywordId,
    into: KeywordId,
) -> Result<Edit, ManageError> {
    let tree = load(connection)?;
    if tree.get(from).is_none() || tree.get(into).is_none() {
        return Err(ManageError::NotFound);
    }
    if tree.is_within(into, from) {
        return Err(ManageError::MergeIntoBranch);
    }
    // `into` peut être un parent de `from` : sa branche contient alors
    // celle de `from`, dont les liens ne doivent être lus qu'une fois.
    let mut involved = tree.branch(into);
    let already: HashSet<KeywordId> = involved.iter().copied().collect();
    involved.extend(
        tree.branch(from)
            .into_iter()
            .filter(|id| !already.contains(id)),
    );
    let links = keywords::links_of_keywords(connection, &involved)?;
    let present: HashSet<(MediaId, KeywordId)> = links.iter().copied().collect();
    let mut media_of: HashMap<KeywordId, Vec<MediaId>> = HashMap::new();
    for &(media, keyword) in &links {
        media_of.entry(keyword).or_default().push(media);
    }
    let mut slots_of: HashMap<KeywordId, Vec<SlotRef>> = HashMap::new();
    for (slot, keyword) in keyword_sets::slots_of_keywords(connection, &tree.branch(from))? {
        slots_of.entry(keyword).or_default().push(slot);
    }
    let mut plan = MergePlan {
        tree: &tree,
        present: &present,
        media_of: &media_of,
        slots_of: &slots_of,
        connection,
        merged: from,
        steps: Vec::new(),
    };
    plan.merge(from, into)?;
    let steps = plan.steps;
    let from_branch: HashSet<KeywordId> = tree.branch(from).into_iter().collect();
    let media = distinct_media(
        links
            .iter()
            .filter(|(_, keyword)| from_branch.contains(keyword)),
    );
    Ok(tree_edit(KeywordTreeChange::Merged, steps, media))
}

/// Prépare la suppression d'un mot-clé et de tous ses descendants ; ils
/// sont retirés des photos qui les portent.
///
/// # Errors
///
/// Voir [`ManageError`].
pub fn delete(connection: &Connection, id: KeywordId) -> Result<Edit, ManageError> {
    let tree = load(connection)?;
    let branch = tree.branch(id);
    if branch.is_empty() {
        return Err(ManageError::NotFound);
    }
    Ok(deletion(connection, &tree, &branch)?)
}

/// Prépare la suppression des mots-clés inutilisés : ceux dont aucune photo
/// ne porte ni le mot-clé ni un de ses descendants.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn delete_unused(connection: &Connection) -> Result<Edit, CatalogError> {
    let tree = load(connection)?;
    let counts = branch_photo_counts(&tree, keywords::all_links(connection)?);
    let unused = |id: KeywordId| !counts.contains_key(&id);
    // Les descendants d'un mot-clé inutilisé le sont aussi : on part des
    // plus hauts, et leur branche entière part avec eux.
    let mut ids = Vec::new();
    for keyword in tree.keywords() {
        if unused(keyword.id) && !keyword.parent_id.is_some_and(unused) {
            ids.extend(tree.branch(keyword.id));
        }
    }
    deletion(connection, &tree, &ids)
}

fn load(connection: &Connection) -> Result<KeywordTree, CatalogError> {
    Ok(KeywordTree::new(keywords::all(connection)?))
}

fn tree_edit(change: KeywordTreeChange, steps: Vec<KeywordTreeStep>, media: Vec<MediaId>) -> Edit {
    Edit::KeywordTree(KeywordTreeEdit::new(change, steps, media))
}

/// Suppression des mots-clés `ids`, donnés chaque parent avant ses enfants
/// (et chaque branche entière) : les liens d'abord, puis les mots-clés, les
/// enfants avant leur parent.
fn deletion(
    connection: &Connection,
    tree: &KeywordTree,
    ids: &[KeywordId],
) -> Result<Edit, CatalogError> {
    let links = keywords::links_of_keywords(connection, ids)?;
    let media = distinct_media(links.iter());
    let mut steps = Vec::with_capacity(ids.len() + 1);
    if !links.is_empty() {
        steps.push(KeywordTreeStep::Unlink(links));
    }
    steps.extend(
        ids.iter()
            .rev()
            .filter_map(|id| tree.get(*id).cloned().map(KeywordTreeStep::Delete)),
    );
    Ok(tree_edit(KeywordTreeChange::Deleted, steps, media))
}

/// Photos qui portent le mot-clé `id` ou un de ses descendants.
fn branch_media(
    connection: &Connection,
    tree: &KeywordTree,
    id: KeywordId,
) -> Result<Vec<MediaId>, CatalogError> {
    let links = keywords::links_of_keywords(connection, &tree.branch(id))?;
    Ok(distinct_media(links.iter()))
}

fn distinct_media<'a>(links: impl Iterator<Item = &'a (MediaId, KeywordId)>) -> Vec<MediaId> {
    let mut seen = HashSet::new();
    links
        .map(|(media, _)| *media)
        .filter(|media| seen.insert(*media))
        .collect()
}
