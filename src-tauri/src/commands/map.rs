//! Vue Carte : groupes de photos
//! géolocalisées dans la zone visible, photos d'un groupe, et position posée
//! ou retirée à la main (annulable).

use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::catalog_info::{CatalogReadFailure, GridSource, read, resolve_query};
use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::grid_filter::GridFilter;
use super::places::InterfaceLanguage;
use crate::catalog::Catalog;
use crate::catalog::repositories::map::{self, GeoArea};
use crate::domain::{GpsPosition, Language};
use crate::edits::{Edit, History, NewLocation};
use crate::places::PlaceIndex;

/// Nombre maximal de photos sélectionnées d'un clic sur un groupe.
const MAX_AREA_IDS: u64 = 10_000;

/// Zone de la carte, en degrés.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MapAreaDto {
    /// Latitude du bord sud.
    pub south: f64,
    /// Longitude du bord ouest.
    pub west: f64,
    /// Latitude du bord nord.
    pub north: f64,
    /// Longitude du bord est.
    pub east: f64,
}

// Reçue en JSON, qui ne sait pas écrire NaN : l'égalité est totale.
impl Eq for MapAreaDto {}

impl MapAreaDto {
    /// Zone valide, ramenée aux bornes du monde.
    #[must_use]
    pub fn area(self) -> Option<GeoArea> {
        GeoArea::new(self.south, self.west, self.north, self.east)
    }
}

impl From<GeoArea> for MapAreaDto {
    fn from(area: GeoArea) -> Self {
        Self {
            south: area.south,
            west: area.west,
            north: area.north,
            east: area.east,
        }
    }
}

/// Groupe de photos proches.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MapClusterDto {
    /// Centre du groupe.
    pub latitude: f64,
    /// Centre du groupe.
    pub longitude: f64,
    /// Nombre de photos.
    pub count: u32,
    /// Une photo du groupe, pour sa vignette.
    pub cover_id: u32,
    /// Zone couverte par ses photos.
    pub area: MapAreaDto,
}

/// Ce que la vue Carte affiche.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MapViewDto {
    /// Groupes de la zone visible.
    pub clusters: Vec<MapClusterDto>,
    /// Photos de la source avec une position (toute la carte).
    pub located_count: u32,
    /// Photos de la source sans position.
    pub unlocated_count: u32,
}

/// Filtre de la grille, sans la limite de zone : la carte montre toujours
/// tout ce qui l'entoure (sinon, la déplacer ne montrerait plus rien).
fn without_area(filter: &GridFilter) -> GridFilter {
    GridFilter {
        area: None,
        ..filter.clone()
    }
}

/// Groupes de photos géolocalisées de la source filtrée dans `area`, par
/// cases de `cell_degrees` degrés.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_map_view(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
    area: MapAreaDto,
    cell_degrees: f64,
) -> Result<MapViewDto, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    let connection = catalog.connection();
    read(|| {
        let Some(query) = resolve_query(connection, source, &without_area(&filter), None)? else {
            return Ok(MapViewDto {
                clusters: Vec::new(),
                located_count: 0,
                unlocated_count: 0,
            });
        };
        let (located, unlocated) = map::located_counts(connection, &query)?;
        let clusters = match area.area() {
            Some(area) => map::clusters(connection, &query, area, cell_degrees)?,
            None => Vec::new(),
        };
        Ok(MapViewDto {
            clusters: clusters
                .into_iter()
                .map(|cluster| MapClusterDto {
                    latitude: cluster.latitude,
                    longitude: cluster.longitude,
                    count: u32::try_from(cluster.count).unwrap_or(u32::MAX),
                    cover_id: u32::try_from(cluster.cover.get()).unwrap_or(u32::MAX),
                    area: cluster.area.into(),
                })
                .collect(),
            located_count: u32::try_from(located).unwrap_or(u32::MAX),
            unlocated_count: u32::try_from(unlocated).unwrap_or(u32::MAX),
        })
    })
}

/// Photos de la source filtrée situées dans `area` (les photos d'un groupe),
/// dans l'ordre de la grille.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_map_area_ids(
    catalog: State<'_, Mutex<Catalog>>,
    source: GridSource,
    filter: GridFilter,
    area: MapAreaDto,
) -> Result<Vec<u32>, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    let connection = catalog.connection();
    read(|| {
        let (Some(query), Some(area)) = (
            resolve_query(connection, source, &without_area(&filter), None)?,
            area.area(),
        ) else {
            return Ok(Vec::new());
        };
        Ok(map::ids_in(connection, &query, area, MAX_AREA_IDS)?
            .into_iter()
            .filter_map(|id| u32::try_from(id.get()).ok())
            .collect())
    })
}

/// Photos à géolocaliser, et combien ont déjà une position ou un lieu (à
/// confirmer avant de les remplacer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationPreviewDto {
    /// Photos concernées.
    pub count: u32,
    /// Parmi elles, celles qui ont déjà une position, un pays, une région
    /// ou une ville : ils seront remplacés.
    pub overwritten: u32,
}

/// Combien des photos `media_ids` ont déjà une position ou un lieu.
///
/// # Errors
///
/// Échoue si le catalogue n'est pas ouvert ou est illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_location_preview(
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<LocationPreviewDto, CatalogReadFailure> {
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    let ids = media_ids_of(media_ids);
    read(|| {
        let (count, overwritten) = map::described_among(catalog.connection(), &ids)?;
        Ok(LocationPreviewDto {
            count: u32::try_from(count).unwrap_or(u32::MAX),
            overwritten: u32::try_from(overwritten).unwrap_or(u32::MAX),
        })
    })
}

/// Donne la position (`latitude`, `longitude`) aux photos `media_ids`, avec
/// le pays, la région et la ville du lieu le plus proche, de façon
/// annulable : ce qu'elles avaient est remplacé (l'interface a demandé
/// confirmation).
///
/// # Errors
///
/// [`EditFailure::InvalidValue`] hors des bornes du monde ;
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
// Tâche asynchrone : la première recherche de lieu lit la base des lieux
// (quelques dixièmes de seconde), sans figer l'interface.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn set_location(
    language: State<'_, InterfaceLanguage>,
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    latitude: f64,
    longitude: f64,
) -> Result<EditOutcome, EditFailure> {
    if !(-90.0..=90.0).contains(&latitude) || !longitude.is_finite() {
        return Err(EditFailure::InvalidValue);
    }
    // La carte peut avoir fait le tour du monde : la longitude y est ramenée.
    let longitude = (longitude + 180.0).rem_euclid(360.0) - 180.0;
    let position = GpsPosition {
        latitude,
        longitude,
        altitude: None,
    };
    let index = PlaceIndex::shared().map_err(|error| {
        tracing::error!(%error, "cannot read the place database");
        EditFailure::CatalogUnavailable
    })?;
    // L'interface annonce sa langue au démarrage, bien avant un glisser.
    let language = language.get().unwrap_or(Language::English);
    let location = NewLocation {
        position,
        place: index.lookup(latitude, longitude, language),
    };
    change_location(&history, &catalog, Some(location), media_ids)
}

/// Retire la position des photos `media_ids`, de façon annulable ; leur
/// lieu (pays, région, ville) est gardé.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub async fn remove_location(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
) -> Result<EditOutcome, EditFailure> {
    change_location(&history, &catalog, None, media_ids)
}

fn change_location(
    history: &Mutex<History>,
    catalog: &Mutex<Catalog>,
    location: Option<NewLocation>,
    media_ids: Vec<u32>,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(history);
    let mut catalog = lock(catalog);
    let edit = Edit::location(&catalog, location, &ids).map_err(|error| failure(&error))?;
    let summary = history
        .perform(&mut catalog, edit)
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}
