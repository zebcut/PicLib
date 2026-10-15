//! Piles : chaque changement est préparé ici,
//! vérifié, puis fait de façon annulable (Cmd+Z). Replier ou déplier une
//! pile ne s'annule pas : c'est un état d'affichage.

use std::collections::{HashMap, HashSet};

use crate::catalog::repositories::stacks::{self, Stack, StackCandidate};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::MediaId;
use crate::edits::{Edit, EditSummary, History, StackChange, StackEdit};

mod auto;

pub use auto::{
    AutoMethod, AutoPlan, AutoScope, BURST_GAP_CHOICES, apply as auto_stack, plan as auto_plan,
    stack_imported_bursts, stack_imported_pairs,
};

/// Changement refusé.
#[derive(Debug, thiserror::Error)]
pub enum StackError {
    /// Moins de deux photos à empiler.
    #[error("at least two photos are needed to make a stack")]
    TooFew,
    /// Les photos ne sont pas toutes dans le même dossier.
    #[error("the photos of a stack must be in the same folder")]
    SeveralFolders,
    /// Aucune des photos n'est dans une pile.
    #[error("none of the photos is in a stack")]
    NotStacked,
    /// L'empilement automatique n'a rien trouvé à empiler.
    #[error("nothing to stack")]
    NothingToStack,
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Empile les photos `ids` (Cmd+G) : une photo déjà dans une pile y emmène
/// toute sa pile, et les piles touchées sont fusionnées. La couverture est
/// celle de la première photo de `ids` (l'interface y met la photo
/// active) ; les autres suivent par date de prise de vue. La pile est
/// repliée.
///
/// # Errors
///
/// Voir [`StackError`] ; rien n'est changé en cas d'échec.
pub fn stack(
    catalog: &mut Catalog,
    history: &mut History,
    ids: &[MediaId],
) -> Result<EditSummary, StackError> {
    let connection = catalog.connection();
    let selected = stacks::candidates(connection, ids)?;
    let touched = stacks::of_media(connection, ids)?;
    let units = units_of(&selected, &touched);
    let members: Vec<MediaId> = units.iter().flatten().copied().collect();
    if members.len() < 2 {
        return Err(StackError::TooFew);
    }
    let all = stacks::candidates(connection, &members)?;
    check_one_folder(&all)?;
    let id = match touched.first() {
        Some(stack) => stack.id,
        None => stacks::next_id(connection)?,
    };
    let stack = Stack {
        id,
        collapsed: true,
        members: ordered(units, &all),
    };
    perform(catalog, history, StackChange::Stacked, touched, vec![stack])
}

/// Défait les piles des photos `ids` (Cmd+Maj+G) : leurs photos restent au
/// catalogue.
///
/// # Errors
///
/// Voir [`StackError`] ; rien n'est changé en cas d'échec.
pub fn unstack(
    catalog: &mut Catalog,
    history: &mut History,
    ids: &[MediaId],
) -> Result<EditSummary, StackError> {
    let touched = stacks::of_media(catalog.connection(), ids)?;
    if touched.is_empty() {
        return Err(StackError::NotStacked);
    }
    perform(
        catalog,
        history,
        StackChange::Unstacked,
        touched,
        Vec::new(),
    )
}

/// Fait de la photo `id` la couverture de sa pile (clic droit ▸ « Définir
/// comme couverture ») ; les autres gardent leur ordre.
///
/// # Errors
///
/// Voir [`StackError`] ; rien n'est changé en cas d'échec.
pub fn set_cover(
    catalog: &mut Catalog,
    history: &mut History,
    id: MediaId,
) -> Result<EditSummary, StackError> {
    let Some(before) = stacks::of_media(catalog.connection(), &[id])?
        .into_iter()
        .find(|stack| stack.members.len() >= 2)
    else {
        return Err(StackError::NotStacked);
    };
    let members = std::iter::once(id)
        .chain(
            before
                .members
                .iter()
                .copied()
                .filter(|member| *member != id),
        )
        .collect();
    let after = Stack {
        members,
        ..before.clone()
    };
    perform(
        catalog,
        history,
        StackChange::CoverChanged,
        vec![before],
        vec![after],
    )
}

/// Retire les photos `ids` de leur pile (clic droit ▸ « Retirer de la
/// pile ») ; une pile réduite à une photo est défaite.
///
/// # Errors
///
/// Voir [`StackError`] ; rien n'est changé en cas d'échec.
pub fn remove_from_stacks(
    catalog: &mut Catalog,
    history: &mut History,
    ids: &[MediaId],
) -> Result<EditSummary, StackError> {
    let touched = stacks::of_media(catalog.connection(), ids)?;
    if touched.is_empty() {
        return Err(StackError::NotStacked);
    }
    let removed: HashSet<_> = ids.iter().copied().collect();
    let after = touched
        .iter()
        .map(|stack| Stack {
            members: stack
                .members
                .iter()
                .copied()
                .filter(|member| !removed.contains(member))
                .collect(),
            ..stack.clone()
        })
        .filter(|stack| stack.members.len() >= 2)
        .collect();
    perform(catalog, history, StackChange::Removed, touched, after)
}

/// Les photos `ids`, plus toutes les photos des piles repliées où elles
/// sont : une note, un marqueur ou une couleur posé sur une pile repliée
/// va à toute la pile si la préférence le demande. Chaque photo
/// une fois, dans l'ordre.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn with_collapsed_members(
    catalog: &Catalog,
    ids: &[MediaId],
) -> Result<Vec<MediaId>, CatalogError> {
    let collapsed: HashMap<MediaId, Vec<MediaId>> = stacks::of_media(catalog.connection(), ids)?
        .into_iter()
        .filter(|stack| stack.collapsed && stack.members.len() >= 2)
        .flat_map(|stack| {
            stack
                .members
                .iter()
                .map(|member| (*member, stack.members.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut seen = HashSet::new();
    let mut all = Vec::with_capacity(ids.len());
    for id in ids {
        let group = collapsed.get(id).map_or_else(|| vec![*id], Clone::clone);
        all.extend(group.into_iter().filter(|member| seen.insert(*member)));
    }
    Ok(all)
}

/// Déplie les piles des photos `ids` si l'une est repliée, les replie
/// sinon (touche S, indicateur de pile) ; renvoie le nouvel état (`None` :
/// aucune pile). Hors de Cmd+Z.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn toggle_collapsed(catalog: &Catalog, ids: &[MediaId]) -> Result<Option<bool>, CatalogError> {
    let connection = catalog.connection();
    let touched: Vec<Stack> = stacks::of_media(connection, ids)?
        .into_iter()
        .filter(|stack| stack.members.len() >= 2)
        .collect();
    if touched.is_empty() {
        return Ok(None);
    }
    let collapsed = !touched.iter().any(|stack| stack.collapsed);
    let ids: Vec<_> = touched.iter().map(|stack| stack.id).collect();
    stacks::set_collapsed(connection, &ids, collapsed)?;
    Ok(Some(collapsed))
}

/// Ce qui rejoint la pile, unité par unité : une photo seule, ou toutes
/// les photos de sa pile, dans l'ordre de `selected`.
fn units_of(selected: &[StackCandidate], touched: &[Stack]) -> Vec<Vec<MediaId>> {
    let by_id: HashMap<_, _> = touched.iter().map(|stack| (stack.id, stack)).collect();
    let mut taken = HashSet::new();
    let mut units = Vec::new();
    for candidate in selected {
        match candidate.stack.and_then(|id| by_id.get(&id)) {
            Some(stack) => {
                if taken.insert(stack.id) {
                    units.push(stack.members.clone());
                }
            }
            None => units.push(vec![candidate.id]),
        }
    }
    units
}

/// La première unité en tête (sa couverture devient celle de la pile),
/// les autres par date de prise de vue puis par nom.
fn ordered(mut units: Vec<Vec<MediaId>>, all: &[StackCandidate]) -> Vec<MediaId> {
    let by_id: HashMap<_, _> = all
        .iter()
        .map(|candidate| (candidate.id, candidate))
        .collect();
    let key = |unit: &Vec<MediaId>| {
        unit.first()
            .and_then(|id| by_id.get(id))
            .map(|candidate| (candidate.capture_time.clone(), candidate.file_name.clone()))
    };
    if units.len() > 1 {
        units[1..].sort_by_key(key);
    }
    units.into_iter().flatten().collect()
}

/// Une pile est propre à un dossier.
pub(crate) fn check_one_folder(all: &[StackCandidate]) -> Result<(), StackError> {
    let folders: HashSet<_> = all.iter().map(|candidate| candidate.folder_id).collect();
    if folders.len() > 1 {
        return Err(StackError::SeveralFolders);
    }
    Ok(())
}

/// Remplace `before` par `after`, de façon annulable.
pub(crate) fn perform(
    catalog: &mut Catalog,
    history: &mut History,
    change: StackChange,
    before: Vec<Stack>,
    after: Vec<Stack>,
) -> Result<EditSummary, StackError> {
    Ok(history.perform(catalog, Edit::Stack(StackEdit::new(change, before, after)))?)
}
