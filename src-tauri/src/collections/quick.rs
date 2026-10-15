//! Collection rapide et collection cible.

use std::collections::HashSet;

use super::{CollectionError, collection, distinct, free_name, perform};
use crate::catalog::Catalog;
use crate::catalog::repositories::collections::{self, CollectionKind, CollectionNode};
use crate::catalog::repositories::quick_collection;
use crate::domain::{CollectionId, MediaId};
use crate::edits::{CollectionChange, CollectionStep, EditSummary, History};

/// Touche B : si toutes les photos sont déjà dans la collection
/// cible, elles en sont retirées ; sinon, celles qui manquent y sont
/// ajoutées. Sans cible, c'est la collection rapide.
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn toggle_in_target(
    catalog: &mut Catalog,
    history: &mut History,
    media: &[MediaId],
) -> Result<EditSummary, CollectionError> {
    let connection = catalog.connection();
    let media = distinct(collections::existing_media(connection, media)?);
    let target = quick_collection::target(connection)?;
    let present = match target {
        Some(id) => collections::present(connection, id, &media)?,
        None => quick_collection::present(connection, &media)?,
    };
    let all_present = !media.is_empty() && present.len() == media.len();
    let (change, step) = match (target, all_present) {
        (Some(collection), true) => (
            CollectionChange::MediaRemoved,
            CollectionStep::Remove {
                collection,
                media: present,
            },
        ),
        (Some(collection), false) => (
            CollectionChange::MediaAdded,
            CollectionStep::Add {
                collection,
                media: missing(media, &present),
            },
        ),
        (None, true) => (
            CollectionChange::QuickRemoved,
            CollectionStep::QuickRemove(present),
        ),
        (None, false) => (
            CollectionChange::QuickAdded,
            CollectionStep::QuickAdd(missing(media, &present)),
        ),
    };
    perform(catalog, history, change, vec![step])
}

/// Vide la collection rapide (Cmd+Maj+B).
///
/// # Errors
///
/// Voir [`CollectionError`].
pub fn clear_quick(
    catalog: &mut Catalog,
    history: &mut History,
) -> Result<EditSummary, CollectionError> {
    let media = quick_collection::media(catalog.connection())?;
    perform(
        catalog,
        history,
        CollectionChange::QuickCleared,
        vec![CollectionStep::QuickRemove(media)],
    )
}

/// Enregistre la collection rapide comme collection, à la racine
/// (Cmd+Option+B), puis la vide si `clear`.
///
/// # Errors
///
/// Voir [`CollectionError`] ; rien n'est créé en cas d'échec.
pub fn save_quick(
    catalog: &mut Catalog,
    history: &mut History,
    name: &str,
    clear: bool,
) -> Result<(CollectionId, EditSummary), CollectionError> {
    let connection = catalog.connection();
    let name = free_name(connection, None, name, None)?;
    let media = quick_collection::media(connection)?;
    let node = CollectionNode {
        id: collections::next_id(connection)?,
        parent_id: None,
        kind: CollectionKind::Collection,
        name,
        rules: None,
    };
    let id = node.id;
    let mut steps = vec![
        CollectionStep::Create(node),
        CollectionStep::Add {
            collection: id,
            media: media.clone(),
        },
    ];
    if clear {
        steps.push(CollectionStep::QuickRemove(media));
    }
    let summary = perform(catalog, history, CollectionChange::QuickSaved, steps)?;
    Ok((id, summary))
}

/// Choisit la collection cible (`None` : la collection rapide). Un
/// réglage retenu dans le catalogue, hors de Cmd+Z.
///
/// # Errors
///
/// Voir [`CollectionError`] : seule une collection peut être la cible.
pub fn set_target(catalog: &Catalog, id: Option<CollectionId>) -> Result<(), CollectionError> {
    if let Some(id) = id {
        collection(catalog.connection(), id)?;
    }
    Ok(quick_collection::set_target(catalog.connection(), id)?)
}

/// Les photos de `media` absentes de `present`.
fn missing(media: Vec<MediaId>, present: &[MediaId]) -> Vec<MediaId> {
    let present: HashSet<MediaId> = present.iter().copied().collect();
    media
        .into_iter()
        .filter(|id| !present.contains(id))
        .collect()
}
