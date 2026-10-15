//! Préréglages de filtres de l'utilisateur : leur contenu,
//! vérifié à la lecture, et leurs changements annulables ; filtre mémorisé
//! pour chaque source (réglage d'affichage, hors de Cmd+Z).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::catalog_info::GridSource;
use super::edits::{EditOutcome, lock, outcome};
use super::grid_filter::{GridFilter, GridMetadataColumn};
use crate::catalog::Catalog;
use crate::catalog::repositories::filter_presets;
use crate::catalog::repositories::source_filters::{self, FilterSource};
use crate::domain::{CollectionId, FilterPresetId, FolderId, KeywordId};
use crate::edits::History;
use crate::filter_presets::{self as presets, PresetError};

/// Onglet de la barre de filtres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridFilterTab {
    /// Texte.
    Text,
    /// Attribut.
    Attribute,
    /// Métadonnées.
    Metadata,
}

/// Ce qu'un préréglage retient : les onglets ouverts, les critères et les
/// colonnes du filtre Métadonnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FilterPresetContent {
    /// Onglets ouverts.
    pub tabs: Vec<GridFilterTab>,
    /// Critères des filtres.
    pub filter: GridFilter,
    /// Colonnes du filtre Métadonnées.
    pub columns: Vec<GridMetadataColumn>,
}

/// Préréglage de l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FilterPresetDto {
    /// Identifiant.
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Contenu.
    pub content: FilterPresetContent,
}

/// Changement de préréglage refusé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FilterPresetFailure {
    /// Nom vide.
    EmptyName,
    /// Nom trop long.
    TooLong,
    /// Un autre préréglage porte déjà ce nom.
    NameTaken,
    /// Le préréglage n'existe plus.
    NotFound,
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
}

impl From<PresetError> for FilterPresetFailure {
    fn from(error: PresetError) -> Self {
        match error {
            PresetError::EmptyName => Self::EmptyName,
            PresetError::TooLong => Self::TooLong,
            PresetError::NameTaken(_) => Self::NameTaken,
            PresetError::NotFound => Self::NotFound,
            PresetError::Catalog(error) => {
                tracing::error!(%error, "cannot change a filter preset");
                Self::CatalogUnavailable
            }
        }
    }
}

/// Préréglage créé et résultat de la modification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FilterPresetCreated {
    /// Identifiant du nouveau préréglage.
    pub id: u32,
    /// Résultat (pour le menu Édition).
    pub outcome: EditOutcome,
}

fn preset_id(id: u32) -> FilterPresetId {
    FilterPresetId::new(i64::from(id))
}

fn serialized(content: &FilterPresetContent) -> Result<String, FilterPresetFailure> {
    serde_json::to_string(content).map_err(|error| {
        tracing::error!(%error, "cannot serialize a filter preset");
        FilterPresetFailure::CatalogUnavailable
    })
}

/// Préréglages de l'utilisateur, par ordre alphabétique. Un contenu
/// illisible (base abîmée, version future) est écarté et journalisé.
///
/// # Errors
///
/// [`FilterPresetFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_filter_presets(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<FilterPresetDto>, FilterPresetFailure> {
    let catalog = lock(&catalog);
    let stored = filter_presets::list(catalog.connection())
        .map_err(|error| FilterPresetFailure::from(PresetError::Catalog(error)))?;
    Ok(stored
        .into_iter()
        .filter_map(|preset| {
            match serde_json::from_str::<FilterPresetContent>(&preset.content) {
                Ok(content) => Some(FilterPresetDto {
                    id: u32::try_from(preset.id.get()).ok()?,
                    name: preset.name,
                    content,
                }),
                Err(error) => {
                    tracing::warn!(%error, id = preset.id.get(), "unreadable filter preset ignored");
                    None
                }
            }
        })
        .collect())
}

/// Crée un préréglage, de façon annulable.
///
/// # Errors
///
/// Voir [`FilterPresetFailure`] ; rien n'est créé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn create_filter_preset(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    name: String,
    content: FilterPresetContent,
) -> Result<FilterPresetCreated, FilterPresetFailure> {
    let content = serialized(&content)?;
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let (id, summary) = presets::create(&mut catalog, &mut history, &name, content)?;
    Ok(FilterPresetCreated {
        id: u32::try_from(id.get()).unwrap_or(u32::MAX),
        outcome: outcome(Some(summary), &history),
    })
}

/// Remplace le contenu d'un préréglage, de façon annulable.
///
/// # Errors
///
/// Voir [`FilterPresetFailure`].
#[tauri::command]
#[specta::specta]
pub async fn update_filter_preset(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
    content: FilterPresetContent,
) -> Result<EditOutcome, FilterPresetFailure> {
    let content = serialized(&content)?;
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = presets::update(&mut catalog, &mut history, preset_id(id), content)?;
    Ok(outcome(Some(summary), &history))
}

/// Renomme un préréglage, de façon annulable.
///
/// # Errors
///
/// Voir [`FilterPresetFailure`].
#[tauri::command]
#[specta::specta]
pub async fn rename_filter_preset(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
    name: String,
) -> Result<EditOutcome, FilterPresetFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = presets::rename(&mut catalog, &mut history, preset_id(id), &name)?;
    Ok(outcome(Some(summary), &history))
}

/// Supprime un préréglage, de façon annulable.
///
/// # Errors
///
/// Voir [`FilterPresetFailure`].
#[tauri::command]
#[specta::specta]
pub async fn delete_filter_preset(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    id: u32,
) -> Result<EditOutcome, FilterPresetFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = presets::delete(&mut catalog, &mut history, preset_id(id))?;
    Ok(outcome(Some(summary), &history))
}

/// Filtre mémorisé d'une source : les onglets ouverts et les critères (les
/// colonnes du filtre Métadonnées restent communes à toutes les sources).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceFilterContent {
    /// Onglets ouverts.
    pub tabs: Vec<GridFilterTab>,
    /// Critères des filtres.
    pub filter: GridFilter,
}

fn filter_source(source: GridSource) -> FilterSource {
    match source {
        GridSource::All => FilterSource::All,
        GridSource::LatestImport => FilterSource::LatestImport,
        GridSource::Keyword { keyword_id } => {
            FilterSource::Keyword(KeywordId::new(i64::from(keyword_id)))
        }
        GridSource::Folder { folder_id } => {
            FilterSource::Folder(FolderId::new(i64::from(folder_id)))
        }
        GridSource::Collection { collection_id } => {
            FilterSource::Collection(CollectionId::new(i64::from(collection_id)))
        }
        GridSource::QuickCollection => FilterSource::QuickCollection,
        GridSource::Missing => FilterSource::Missing,
    }
}

/// Filtre mémorisé d'une source ; `null` s'il n'y en a pas, ou s'il est
/// illisible (journalisé).
///
/// # Errors
///
/// [`FilterPresetFailure::CatalogUnavailable`] si le catalogue est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_source_filter(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
) -> Result<Option<SourceFilterContent>, FilterPresetFailure> {
    let catalog = lock(&catalog);
    let stored = source_filters::get(catalog.connection(), filter_source(source))
        .map_err(|error| FilterPresetFailure::from(PresetError::Catalog(error)))?;
    Ok(stored.and_then(|content| {
        serde_json::from_str(&content)
            .inspect_err(|error| tracing::warn!(%error, "unreadable source filter ignored"))
            .ok()
    }))
}

/// Mémorise le filtre d'une source (`null` : l'oublie).
///
/// # Errors
///
/// [`FilterPresetFailure::CatalogUnavailable`] si le catalogue ne peut pas
/// être écrit.
#[tauri::command]
#[specta::specta]
pub async fn save_source_filter(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    content: Option<SourceFilterContent>,
) -> Result<(), FilterPresetFailure> {
    let content = content
        .map(|content| {
            serde_json::to_string(&content).map_err(|error| {
                tracing::error!(%error, "cannot serialize a source filter");
                FilterPresetFailure::CatalogUnavailable
            })
        })
        .transpose()?;
    let catalog = lock(&catalog);
    source_filters::put(
        catalog.connection(),
        filter_source(source),
        content.as_deref(),
    )
    .map_err(|error| FilterPresetFailure::from(PresetError::Catalog(error)))
}
