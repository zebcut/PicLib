//! Piles : empiler, dépiler, changer la couverture, retirer une
//! photo, empiler automatiquement, replier et déplier.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::edits::{EditOutcome, lock, media_ids_of, outcome};
use crate::catalog::Catalog;
use crate::domain::{FolderId, MediaId};
use crate::edits::History;
use crate::stacks::{self, AutoMethod, AutoScope, BURST_GAP_CHOICES, StackError};

/// Changement demandé.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StackOperation {
    /// Empiler (Cmd+G) : la première photo donne la couverture.
    #[serde(rename_all = "camelCase")]
    Stack {
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Dépiler (Cmd+Maj+G).
    #[serde(rename_all = "camelCase")]
    Unstack {
        /// Photos dont les piles sont défaites.
        media_ids: Vec<u32>,
    },
    /// Faire de cette photo la couverture de sa pile.
    #[serde(rename_all = "camelCase")]
    SetCover {
        /// Photo.
        media_id: u32,
    },
    /// Retirer ces photos de leur pile.
    #[serde(rename_all = "camelCase")]
    Remove {
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Empiler automatiquement (paires RAW + JPEG, rafales), après
    /// confirmation.
    #[serde(rename_all = "camelCase")]
    Auto {
        /// Photos examinées.
        scope: AutoStackScope,
        /// Façon d'empiler.
        method: AutoStackMethod,
    },
}

/// Photos examinées par l'empilement automatique.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AutoStackScope {
    /// Les photos sélectionnées (et, pour une paire, l'autre fichier).
    #[serde(rename_all = "camelCase")]
    Media {
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Un dossier et ses sous-dossiers.
    #[serde(rename_all = "camelCase")]
    Folder {
        /// Dossier.
        folder_id: u32,
    },
}

impl From<AutoStackScope> for AutoScope {
    fn from(scope: AutoStackScope) -> Self {
        match scope {
            AutoStackScope::Media { media_ids } => Self::Media(media_ids_of(media_ids)),
            AutoStackScope::Folder { folder_id } => {
                Self::Folder(FolderId::new(i64::from(folder_id)))
            }
        }
    }
}

/// Façon d'empiler automatiquement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AutoStackMethod {
    /// Les paires RAW + JPEG.
    Pairs,
    /// Les rafales.
    #[serde(rename_all = "camelCase")]
    Bursts {
        /// Écart maximal entre deux photos, en secondes (une des valeurs
        /// proposées, sinon 2).
        gap_seconds: u32,
    },
}

impl From<AutoStackMethod> for AutoMethod {
    fn from(method: AutoStackMethod) -> Self {
        match method {
            AutoStackMethod::Pairs => Self::Pairs,
            AutoStackMethod::Bursts { gap_seconds } => Self::Bursts {
                gap_seconds: if BURST_GAP_CHOICES.contains(&gap_seconds) {
                    gap_seconds
                } else {
                    2
                },
            },
        }
    }
}

/// Ce qu'un empilement automatique ferait, pour le confirmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoStackPreview {
    /// Piles créées.
    pub stacks: u32,
    /// Photos empilées.
    pub photos: u32,
}

/// Changement refusé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StackFailure {
    /// Moins de deux photos.
    TooFew,
    /// Photos de plusieurs dossiers.
    SeveralFolders,
    /// Aucune photo dans une pile.
    NotStacked,
    /// Rien à empiler automatiquement.
    NothingToStack,
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
}

impl From<StackError> for StackFailure {
    fn from(error: StackError) -> Self {
        match error {
            StackError::TooFew => Self::TooFew,
            StackError::SeveralFolders => Self::SeveralFolders,
            StackError::NotStacked => Self::NotStacked,
            StackError::NothingToStack => Self::NothingToStack,
            StackError::Catalog(error) => {
                tracing::error!(%error, "cannot change the stacks");
                Self::CatalogUnavailable
            }
        }
    }
}

/// Empile ou dépile, de façon annulable.
///
/// # Errors
///
/// Voir [`StackFailure`] ; rien n'est changé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn change_stacks(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    operation: StackOperation,
) -> Result<EditOutcome, StackFailure> {
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let (catalog, history) = (&mut *catalog, &mut *history);
    let summary = match operation {
        StackOperation::Stack { media_ids } => {
            stacks::stack(catalog, history, &media_ids_of(media_ids))?
        }
        StackOperation::Unstack { media_ids } => {
            stacks::unstack(catalog, history, &media_ids_of(media_ids))?
        }
        StackOperation::SetCover { media_id } => {
            stacks::set_cover(catalog, history, MediaId::new(i64::from(media_id)))?
        }
        StackOperation::Remove { media_ids } => {
            stacks::remove_from_stacks(catalog, history, &media_ids_of(media_ids))?
        }
        StackOperation::Auto { scope, method } => {
            stacks::auto_stack(catalog, history, &scope.into(), method.into())?
        }
    };
    Ok(outcome(Some(summary), history))
}

/// Ce qu'un empilement automatique ferait (nombre de piles et de photos),
/// pour le résumé à confirmer.
///
/// # Errors
///
/// [`StackFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_auto_stacking(
    catalog: State<'_, Mutex<Catalog>>,
    scope: AutoStackScope,
    method: AutoStackMethod,
) -> Result<AutoStackPreview, StackFailure> {
    let catalog = lock(&catalog);
    let plan = stacks::auto_plan(catalog.connection(), &scope.into(), method.into())
        .map_err(|error| StackFailure::from(StackError::Catalog(error)))?;
    Ok(AutoStackPreview {
        stacks: u32::try_from(plan.stack_count()).unwrap_or(u32::MAX),
        photos: u32::try_from(plan.photo_count()).unwrap_or(u32::MAX),
    })
}

/// Déplie les piles des photos indiquées si l'une est repliée, les replie
/// sinon ; renvoie le nouvel état (`null` : aucune pile). Hors de Cmd+Z.
///
/// # Errors
///
/// [`StackFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn toggle_stacks(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<Option<bool>, StackFailure> {
    let catalog = lock(&catalog);
    stacks::toggle_collapsed(&catalog, &media_ids_of(media_ids))
        .map_err(|error| StackFailure::from(StackError::Catalog(error)))
}
