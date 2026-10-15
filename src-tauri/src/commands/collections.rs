//! Collections manuelles, dossiers de collections, collection rapide et
//! collection cible(10.3 et 10.4) : le panneau Collections et
//! ses changements annulables.

use std::sync::Mutex;

use tauri::State;

pub use super::collection_types::{
    CollectionChangeDto, CollectionDeletionDto, CollectionDto, CollectionFailure,
    CollectionKindDto, CollectionOperation, QuickCollectionDto,
};
use super::edits::{lock, outcome};
use crate::catalog::Catalog;
use crate::catalog::repositories::collections::{self as repository, CollectionKind};
use crate::catalog::repositories::library::{self, LibrarySource};
use crate::catalog::repositories::quick_collection;
use crate::catalog::repositories::smart_rules::{self, SmartRule};
use crate::collections::{self, CollectionError};
use crate::domain::{CollectionId, MediaId};
use crate::edits::History;

fn collection_id(id: u32) -> CollectionId {
    CollectionId::new(i64::from(id))
}

fn media_ids(ids: &[u32]) -> Vec<MediaId> {
    ids.iter().map(|id| MediaId::new(i64::from(*id))).collect()
}

fn serialized(rules: &SmartRule) -> Result<String, CollectionFailure> {
    serde_json::to_string(rules).map_err(|error| {
        tracing::error!(%error, "cannot serialize smart collection rules");
        CollectionFailure::CatalogUnavailable
    })
}

fn small(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Arbre des collections, avec le nombre de photos de chacune.
///
/// # Errors
///
/// [`CollectionFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_collections(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<CollectionDto>, CollectionFailure> {
    let catalog = lock(&catalog);
    let read = || {
        let connection = catalog.connection();
        let mut counts: std::collections::HashMap<_, _> =
            repository::media_counts(connection)?.into_iter().collect();
        let nodes = repository::list(connection)?;
        // Une collection dynamique, et chaque dossier qui en contient une :
        // leurs photos sont calculées, comme dans la grille.
        let parents: std::collections::HashMap<_, _> =
            nodes.iter().map(|node| (node.id, node.parent_id)).collect();
        let mut computed = std::collections::HashSet::new();
        for node in nodes.iter().filter(|node| node.is_smart()) {
            let mut current = Some(node.id);
            while let Some(id) = current {
                if !computed.insert(id) {
                    break;
                }
                current = parents.get(&id).copied().flatten();
            }
        }
        for id in computed {
            counts.insert(
                id,
                library::count_media(connection, LibrarySource::Collection(id))?,
            );
        }
        Ok(nodes
            .into_iter()
            .map(|node| CollectionDto {
                id: small(node.id.get()),
                parent_id: node.parent_id.map(|parent| small(parent.get())),
                kind: match node.kind {
                    CollectionKind::Collection => CollectionKindDto::Collection,
                    CollectionKind::Folder => CollectionKindDto::Folder,
                },
                media_count: counts.get(&node.id).copied().unwrap_or(0),
                rules: node.rules.as_deref().and_then(smart_rules::parse),
                name: node.name,
            })
            .collect())
    };
    read().map_err(|error| CollectionFailure::from(CollectionError::Catalog(error)))
}

/// Ce qu'emporterait la suppression d'une collection ou d'un dossier.
///
/// # Errors
///
/// Voir [`CollectionFailure`].
#[tauri::command]
#[specta::specta]
pub async fn get_collection_deletion(
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
) -> Result<CollectionDeletionDto, CollectionFailure> {
    let catalog = lock(&catalog);
    let summary = collections::deletion_summary(&catalog, collection_id(id))?;
    Ok(CollectionDeletionDto {
        collections: u32::try_from(summary.collections).unwrap_or(u32::MAX),
        folders: u32::try_from(summary.folders).unwrap_or(u32::MAX),
    })
}

/// Crée, renomme, déplace, supprime une collection ou un dossier, ou y
/// ajoute ou en retire des photos, de façon annulable.
///
/// # Errors
///
/// Voir [`CollectionFailure`] ; rien n'est changé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn change_collections(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    operation: CollectionOperation,
) -> Result<CollectionChangeDto, CollectionFailure> {
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let (catalog, history) = (&mut *catalog, &mut *history);
    let mut created_id = None;
    let summary = match operation {
        CollectionOperation::Create {
            parent_id,
            collection_kind,
            name,
            media_ids: ids,
        } => {
            let (id, summary) = collections::create(
                catalog,
                history,
                parent_id.map(collection_id),
                collection_kind.into(),
                &name,
                &media_ids(&ids),
            )?;
            created_id = Some(small(id.get()));
            summary
        }
        CollectionOperation::CreateSmart {
            parent_id,
            name,
            rules,
        } => {
            let (id, summary) = collections::create_smart(
                catalog,
                history,
                parent_id.map(collection_id),
                &name,
                serialized(&rules)?,
            )?;
            created_id = Some(small(id.get()));
            summary
        }
        CollectionOperation::SetRules {
            collection_id: id,
            rules,
        } => collections::set_rules(catalog, history, collection_id(id), serialized(&rules)?)?,
        CollectionOperation::Rename {
            collection_id: id,
            name,
        } => collections::rename(catalog, history, collection_id(id), &name)?,
        CollectionOperation::Move {
            collection_id: id,
            parent_id,
        } => collections::move_to(
            catalog,
            history,
            collection_id(id),
            parent_id.map(collection_id),
        )?,
        CollectionOperation::Delete { collection_id: id } => {
            collections::delete(catalog, history, collection_id(id))?
        }
        CollectionOperation::AddMedia {
            collection_id: id,
            media_ids: ids,
        } => collections::add_media(catalog, history, collection_id(id), &media_ids(&ids))?,
        CollectionOperation::RemoveMedia {
            collection_id: id,
            media_ids: ids,
        } => collections::remove_media(catalog, history, collection_id(id), &media_ids(&ids))?,
        CollectionOperation::ToggleInTarget { media_ids: ids } => {
            collections::toggle_in_target(catalog, history, &media_ids(&ids))?
        }
        CollectionOperation::ClearQuick => collections::clear_quick(catalog, history)?,
        CollectionOperation::SaveQuick { name, clear } => {
            let (id, summary) = collections::save_quick(catalog, history, &name, clear)?;
            created_id = Some(small(id.get()));
            summary
        }
    };
    Ok(CollectionChangeDto {
        created_id,
        outcome: outcome(Some(summary), history),
    })
}

/// Nombre de photos de la collection rapide, et collection cible.
///
/// # Errors
///
/// [`CollectionFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_quick_collection(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<QuickCollectionDto, CollectionFailure> {
    let catalog = lock(&catalog);
    let read = || {
        Ok(QuickCollectionDto {
            media_count: quick_collection::count(catalog.connection())?,
            target_id: quick_collection::target(catalog.connection())?.map(|id| small(id.get())),
        })
    };
    read().map_err(|error| CollectionFailure::from(CollectionError::Catalog(error)))
}

/// Choisit la collection cible (`null` : la collection rapide) ; un réglage,
/// hors de Cmd+Z.
///
/// # Errors
///
/// Voir [`CollectionFailure`] : seule une collection peut être la cible.
#[tauri::command]
#[specta::specta]
pub async fn set_collection_target(
    catalog: State<'_, Mutex<Catalog>>,
    id: Option<u32>,
) -> Result<(), CollectionFailure> {
    let catalog = lock(&catalog);
    Ok(collections::set_target(&catalog, id.map(collection_id))?)
}
