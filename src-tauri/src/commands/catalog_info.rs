//! Lecture du catalogue ouvert : chiffres généraux et pages de la grille.

use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::catalog::repositories::library::{
    self, LibraryFilter, LibraryItem, LibraryQuery, LibrarySort, LibrarySource, ThumbnailState,
    XmpState,
};
use crate::catalog::repositories::media;
use crate::catalog::repositories::missing;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{CollectionId, FolderId, KeywordId, MediaId, MediaKind};

use super::edits::{ColorLabelDto, FlagDto};
pub use super::grid_filter::{GridColumnValue, GridFilter, GridMetadataColumn, GridSort};
use super::preferences::AppPreferences;

/// Nombre maximal d'éléments par page : au-delà, la demande est ramenée à
/// cette taille (l'interface ne demande que ce qui est visible).
const MAX_PAGE_SIZE: u32 = 500;

/// Contenu du catalogue en quelques chiffres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogOverview {
    /// Nombre de photos.
    #[specta(type = u32)]
    pub photo_count: u64,
    /// Nombre de vidéos.
    #[specta(type = u32)]
    pub video_count: u64,
    /// Nombre d'éléments du dernier import (« Import précédent »), s'il y en
    /// a un.
    #[specta(type = Option<u32>)]
    pub latest_import_items: Option<u64>,
    /// Nombre de photos et vidéos dont le fichier est introuvable.
    #[specta(type = u32)]
    pub missing_count: u64,
}

/// Le catalogue n'a pas pu être lu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CatalogReadFailure {
    /// Aucun catalogue n'est ouvert, ou il est illisible.
    CatalogUnavailable,
}

/// Ensemble d'éléments à afficher dans la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GridSource {
    /// Tout le catalogue.
    All,
    /// Le dernier import (« Import précédent »).
    LatestImport,
    /// Les photos qui portent un mot-clé ou un de ses descendants.
    #[serde(rename_all = "camelCase")]
    Keyword {
        /// Mot-clé.
        keyword_id: u32,
    },
    /// Les photos d'un dossier et de ses sous-dossiers (panneau Dossiers).
    #[serde(rename_all = "camelCase")]
    Folder {
        /// Dossier.
        folder_id: u32,
    },
    /// Les photos d'une collection, ou de toutes les collections d'un
    /// dossier de collections (panneau Collections).
    #[serde(rename_all = "camelCase")]
    Collection {
        /// Collection ou dossier de collections.
        collection_id: u32,
    },
    /// La collection rapide.
    QuickCollection,
    /// Les photos dont le fichier est introuvable (panneau Catalogue).
    Missing,
}

/// État de la vignette d'un élément.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridThumbnail {
    /// Pas encore fabriquée.
    Pending,
    /// Prête : adresse `piclib://thumbnail/<id>`.
    Ready,
    /// Impossible à fabriquer.
    Failed,
}

/// Élément de la grille.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridItem {
    /// Identifiant du média (nombre exact jusqu'à 2⁵³ côté interface).
    #[specta(type = u32)]
    pub id: i64,
    /// Vrai pour une vidéo.
    pub is_video: bool,
    /// Nom du fichier.
    pub file_name: String,
    /// Date de prise de vue locale (`2024-07-14T18:30:05`).
    pub capture_time: String,
    /// Durée d'une vidéo, en millisecondes.
    #[specta(type = Option<u32>)]
    pub duration_ms: Option<u64>,
    /// État de la vignette.
    pub thumbnail: GridThumbnail,
    /// Marqueur.
    pub flag: Option<FlagDto>,
    /// Note, de 0 à 5 étoiles.
    pub rating: u8,
    /// Étiquette de couleur.
    pub color_label: Option<ColorLabelDto>,
    /// Rotation choisie par l'utilisateur, en degrés dans le sens horaire
    /// (0, 90, 180 ou 270), à appliquer à l'affichage.
    pub rotation: u16,
    /// Dans la collection cible (la collection rapide, par défaut).
    pub in_target: bool,
    /// Pile de la photo ; `null` hors d'une pile.
    pub stack: Option<GridStack>,
    /// Fichier introuvable à la dernière vérification.
    pub missing: bool,
    /// État de son fichier XMP ; `null` : à jour (« à enregistrer » n'est
    /// pas montré quand l'écriture est automatique).
    pub xmp: Option<GridXmpState>,
    /// Version de l'image : change quand le fichier est remplacé (export réécrit par un éditeur), pour que l'interface ne garde
    /// pas l'ancienne vignette en mémoire. Millisecondes (nombre exact côté
    /// interface jusqu'à 2⁵³).
    // Annoncé comme un simple nombre (`u32` n'en change que le type
    // TypeScript, comme pour `id`).
    #[specta(type = u32)]
    pub image_version: i64,
}

/// État du fichier XMP d'une photo de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridXmpState {
    /// Métadonnées à enregistrer dans le fichier (écriture manuelle).
    Pending,
    /// Fichier modifié par un autre logiciel.
    ChangedElsewhere,
    /// Dernière écriture échouée.
    Failed,
}

/// Place d'une photo dans sa pile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridStack {
    /// Pile.
    #[specta(type = u32)]
    pub id: i64,
    /// Repliée : la grille n'en montre qu'une photo.
    pub collapsed: bool,
    /// Nombre de photos de la pile.
    pub size: u32,
    /// Rang de la photo dans la pile, à partir de 1 (1 : la couverture).
    pub position: u32,
}

impl From<LibraryItem> for GridItem {
    fn from(item: LibraryItem) -> Self {
        Self {
            id: item.id.get(),
            is_video: item.kind == MediaKind::Video,
            file_name: item.file_name,
            capture_time: item.capture_time,
            duration_ms: item.duration_ms,
            thumbnail: match item.thumbnail {
                ThumbnailState::Pending => GridThumbnail::Pending,
                ThumbnailState::Ready => GridThumbnail::Ready,
                ThumbnailState::Failed => GridThumbnail::Failed,
            },
            flag: item.flag.map(FlagDto::from),
            rating: item.rating.stars(),
            color_label: item.color_label.map(ColorLabelDto::from),
            rotation: item.rotation.degrees(),
            in_target: item.in_target,
            stack: item.stack.map(|stack| GridStack {
                id: stack.id.get(),
                collapsed: stack.collapsed,
                size: stack.size,
                position: stack.position,
            }),
            missing: item.missing,
            xmp: item.xmp.map(|state| match state {
                XmpState::Pending => GridXmpState::Pending,
                XmpState::ChangedElsewhere => GridXmpState::ChangedElsewhere,
                XmpState::Failed => GridXmpState::Failed,
            }),
            image_version: item.file_modified_at.unix_millis(),
        }
    }
}

/// Renvoie le nombre de photos, de vidéos et d'éléments du dernier import.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_catalog_overview(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<CatalogOverview, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        let connection = catalog.connection();
        let counts = media::count_by_kind(connection)?;
        let latest_import_items = library::latest_import(connection)?
            .map(|import| library::count_media(connection, LibrarySource::Import(import)))
            .transpose()?;
        Ok(CatalogOverview {
            photo_count: counts.photos,
            video_count: counts.videos,
            latest_import_items,
            missing_count: missing::count(connection)?,
        })
    })
}

/// Renvoie le nombre d'éléments d'une source de la grille, filtrés.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_grid_count(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
) -> Result<u32, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        let query = resolve_query(catalog.connection(), source, &filter, None)?;
        let count = match query {
            Some(query) => library::count(catalog.connection(), query)?,
            None => 0,
        };
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    })
}

/// Renvoie une page d'éléments de la grille, filtrés et triés.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_grid_page(
    catalog: State<'_, Mutex<Catalog>>,
    preferences: State<'_, AppPreferences>,
    source: GridSource,
    filter: GridFilter,
    sort: GridSort,
    offset: u32,
    limit: u32,
) -> Result<Vec<GridItem>, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        let Some(query) = resolve_query(catalog.connection(), source, &filter, Some(sort))? else {
            return Ok(Vec::new());
        };
        let items = library::page(
            catalog.connection(),
            query,
            u64::from(offset),
            u64::from(limit.min(MAX_PAGE_SIZE)),
        )?;
        // En écriture automatique, « à enregistrer » ne dure que quelques
        // secondes : rien à signaler.
        let automatic = preferences.current().write_xmp_automatically;
        Ok(items
            .into_iter()
            .map(GridItem::from)
            .map(|mut item| {
                if automatic && item.xmp == Some(GridXmpState::Pending) {
                    item.xmp = None;
                }
                item
            })
            .collect())
    })
}

/// Renvoie les identifiants des éléments `offset` à `offset + limit - 1`
/// d'une source, filtrés, dans l'ordre de la grille (sélection d'une plage,
/// ou de tout).
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_grid_ids(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
    sort: GridSort,
    offset: u32,
    limit: u32,
) -> Result<Vec<u32>, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        let Some(query) = resolve_query(catalog.connection(), source, &filter, Some(sort))? else {
            return Ok(Vec::new());
        };
        let ids = library::ids(
            catalog.connection(),
            query,
            u64::from(offset),
            u64::from(limit),
        )?;
        // Les identifiants sont des entiers positifs attribués par SQLite ;
        // ils restent sous 2³² tant que le catalogue a moins de 4 milliards
        // d'éléments.
        Ok(ids
            .into_iter()
            .filter_map(|id| u32::try_from(id.get()).ok())
            .collect())
    })
}

/// Renvoie les valeurs d'une colonne du filtre Métadonnées et leur nombre
/// de médias, parmi ceux de la source filtrés par `filter` (dont les choix
/// des colonnes à sa gauche).
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_metadata_column(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
    column: GridMetadataColumn,
) -> Result<Vec<GridColumnValue>, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    read(|| {
        let Some(query) = resolve_query(catalog.connection(), source, &filter, None)? else {
            return Ok(Vec::new());
        };
        let values = library::column_values(catalog.connection(), &query, column.into())?;
        Ok(values.into_iter().map(GridColumnValue::from).collect())
    })
}

/// Traduit la source, le filtre et le tri demandés par l'interface (le tri
/// par défaut si `sort` est absent) ; `None` s'il n'y a pas encore
/// d'import.
pub(super) fn resolve_query(
    connection: &rusqlite::Connection,
    source: GridSource,
    filter: &GridFilter,
    sort: Option<GridSort>,
) -> Result<Option<LibraryQuery>, CatalogError> {
    Ok(
        resolve_source(connection, source)?.map(|source| LibraryQuery {
            source,
            filter: LibraryFilter::from(filter),
            sort: sort.map(LibrarySort::from).unwrap_or_default(),
        }),
    )
}

/// Traduit la source demandée par l'interface ; `None` s'il n'y a pas encore
/// d'import.
fn resolve_source(
    connection: &rusqlite::Connection,
    source: GridSource,
) -> Result<Option<LibrarySource>, CatalogError> {
    Ok(match source {
        GridSource::All => Some(LibrarySource::All),
        GridSource::LatestImport => library::latest_import(connection)?.map(LibrarySource::Import),
        GridSource::Keyword { keyword_id } => Some(LibrarySource::Keyword(KeywordId::new(
            i64::from(keyword_id),
        ))),
        GridSource::Folder { folder_id } => {
            Some(LibrarySource::Folder(FolderId::new(i64::from(folder_id))))
        }
        GridSource::Collection { collection_id } => Some(LibrarySource::Collection(
            CollectionId::new(i64::from(collection_id)),
        )),
        GridSource::QuickCollection => Some(LibrarySource::QuickCollection),
        GridSource::Missing => Some(LibrarySource::Missing),
    })
}

/// Exécute une lecture et journalise son échec éventuel.
pub(super) fn read<T>(
    action: impl FnOnce() -> Result<T, CatalogError>,
) -> Result<T, CatalogReadFailure> {
    action().map_err(|error| {
        tracing::error!(%error, "cannot read the catalog");
        CatalogReadFailure::CatalogUnavailable
    })
}

/// Disponibilité du fichier d'un média, pour expliquer pourquoi il ne
/// s'affiche pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MediaFileStatus {
    /// Le fichier est présent : s'il ne s'affiche pas, il est illisible.
    Available,
    /// Le fichier est introuvable (disque débranché, fichier déplacé).
    Missing,
    /// Le média n'est plus au catalogue.
    NotInCatalog,
}

/// Indique si le fichier d'un média est présent sur le disque.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn check_media_file(
    catalog: State<'_, Mutex<Catalog>>,
    media_id: u32,
) -> Result<MediaFileStatus, CatalogReadFailure> {
    let file = {
        let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
        read(|| library::media_file(catalog.connection(), MediaId::new(i64::from(media_id))))?
    };
    Ok(match file {
        None => MediaFileStatus::NotInCatalog,
        Some(file) if file.path.is_file() => MediaFileStatus::Available,
        Some(_) => MediaFileStatus::Missing,
    })
}
