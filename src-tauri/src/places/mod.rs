//! Lieu d'une photo retrouvé à partir de sa position GPS, sans connexion
//! (« géocodage inverse »), et remplissage automatique des champs Pays,
//! Région et Ville du catalogue.
//!
//! La base des lieux (lieux habités de plus de 500 habitants, GeoNames,
//! licence CC BY 4.0) est intégrée à l'application : les positions ne
//! quittent jamais l'ordinateur. Elle est fabriquée par
//! `scripts/build-places.sh`.

mod data;
mod fill;

use std::collections::HashMap;
use std::f64::consts::PI;
use std::ops::Range;
use std::sync::OnceLock;

pub use data::PlaceDataError;
pub use fill::{FillError, FillReport, fill_pending};

use crate::domain::{Language, PlaceNames};

/// Au-delà de cette distance du lieu habité le plus proche (en mer, en
/// pleine nature…), aucun lieu n'est proposé plutôt qu'un lieu faux.
pub const MAX_DISTANCE_KM: f64 = 30.0;

/// Rayon moyen de la Terre.
const EARTH_RADIUS_KM: f64 = 6_371.0;

/// Longueur d'un degré de latitude (et de longitude à l'équateur).
const KM_PER_DEGREE: f64 = EARTH_RADIUS_KM * PI / 180.0;

/// Nombre de colonnes d'un degré de longitude autour de la Terre.
const COLUMNS: i32 = 360;

/// Case d'un degré sur un degré (latitude, longitude arrondies vers le bas) :
/// l'index ne compare une position qu'aux lieux des cases voisines, au lieu
/// des 225 000 lieux de la base.
type Cell = (i32, i32);

/// Pays de la base.
#[derive(Debug)]
struct Country {
    french: &'static str,
    english: &'static str,
}

/// Région d'un pays : un groupe de lieux de la base.
#[derive(Debug)]
struct Area {
    /// Position dans la liste des pays.
    country: usize,
    region: Option<&'static str>,
}

/// Lieu habité. Les coordonnées en `f32` sont précises au mètre près, et
/// divisent par deux la mémoire occupée.
#[derive(Debug)]
struct Place {
    latitude: f32,
    longitude: f32,
    /// Position dans la liste des régions.
    area: usize,
    name: &'static str,
}

/// Index de la base des lieux, pour trouver rapidement le lieu le plus
/// proche d'une position.
#[derive(Debug)]
pub struct PlaceIndex {
    countries: Vec<Country>,
    areas: Vec<Area>,
    /// Lieux classés par case.
    places: Vec<Place>,
    /// Lieux de chaque case non vide, dans `places`.
    cells: HashMap<Cell, Range<usize>>,
}

impl PlaceIndex {
    /// Lit la base des lieux intégrée à l'application et construit l'index
    /// (quelques dixièmes de seconde : à faire dans une tâche de fond).
    ///
    /// # Errors
    ///
    /// [`PlaceDataError`] si la base intégrée est mal formée (erreur de
    /// fabrication, vérifiée par un test).
    pub fn load() -> Result<Self, PlaceDataError> {
        Ok(Self::build(data::parse(&data::EMBEDDED)?))
    }

    /// Index gardé en mémoire après la première lecture : une photo placée
    /// sur la carte reçoit son lieu tout de suite (quelques mégaoctets).
    ///
    /// # Errors
    ///
    /// Voir [`Self::load`].
    pub fn shared() -> Result<&'static Self, PlaceDataError> {
        static SHARED: OnceLock<PlaceIndex> = OnceLock::new();
        if let Some(index) = SHARED.get() {
            return Ok(index);
        }
        let index = Self::load()?;
        // Deux premières lectures simultanées : la seconde est oubliée.
        Ok(SHARED.get_or_init(|| index))
    }

    fn build(parsed: data::Parsed) -> Self {
        let data::Parsed {
            countries,
            areas,
            mut places,
        } = parsed;
        places.sort_by_key(|place| cell_of(f64::from(place.latitude), f64::from(place.longitude)));
        let mut cells: HashMap<Cell, Range<usize>> = HashMap::new();
        for (index, place) in places.iter().enumerate() {
            let cell = cell_of(f64::from(place.latitude), f64::from(place.longitude));
            cells
                .entry(cell)
                .and_modify(|range| range.end = index + 1)
                .or_insert(index..index + 1);
        }
        Self {
            countries,
            areas,
            places,
            cells,
        }
    }

    /// Nombre de lieux de la base.
    #[must_use]
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Vrai si la base ne contient aucun lieu.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Pays, région et ville du lieu habité le plus proche de la position,
    /// s'il est à moins de [`MAX_DISTANCE_KM`]. Une position impossible
    /// (latitude au-delà de 90°…) ne donne rien.
    #[must_use]
    pub fn lookup(&self, latitude: f64, longitude: f64, language: Language) -> Option<PlaceNames> {
        if !((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)) {
            return None;
        }
        let place = self.nearest(latitude, longitude)?;
        let area = self.areas.get(place.area)?;
        let country = self.countries.get(area.country)?;
        Some(PlaceNames {
            country: match language {
                Language::French => country.french,
                Language::English => country.english,
            }
            .to_owned(),
            region: area.region.map(str::to_owned),
            city: place.name.to_owned(),
        })
    }

    fn nearest(&self, latitude: f64, longitude: f64) -> Option<&Place> {
        let latitude_span = MAX_DISTANCE_KM / KM_PER_DEGREE;
        let rows = degree_cell((latitude - latitude_span).max(-90.0))
            ..=degree_cell((latitude + latitude_span).min(90.0));
        let mut best: Option<(f64, &Place)> = None;
        for row in rows {
            for column in columns_around(latitude, longitude, latitude_span) {
                let Some(range) = self.cells.get(&(row, column)) else {
                    continue;
                };
                for place in &self.places[range.clone()] {
                    let distance = distance_km(
                        latitude,
                        longitude,
                        f64::from(place.latitude),
                        f64::from(place.longitude),
                    );
                    if distance <= MAX_DISTANCE_KM
                        && best.is_none_or(|(closest, _)| distance < closest)
                    {
                        best = Some((distance, place));
                    }
                }
            }
        }
        best.map(|(_, place)| place)
    }
}

/// Colonnes de longitude à parcourir autour d'une position. Les méridiens
/// se rapprochent vers les pôles : un degré de longitude y couvre moins de
/// kilomètres, il faut donc parcourir plus de colonnes. Le méridien 180°
/// est traversé (îles Fidji…).
fn columns_around(latitude: f64, longitude: f64, latitude_span: f64) -> Vec<i32> {
    // Le parallèle le plus proche du pôle dans la zone parcourue est le plus
    // court : c'est lui qui fixe le nombre de colonnes.
    let narrowest = (latitude.abs() + latitude_span)
        .min(90.0)
        .to_radians()
        .cos()
        * KM_PER_DEGREE;
    let longitude_span = MAX_DISTANCE_KM / narrowest;
    if !longitude_span.is_finite() || longitude_span >= 180.0 {
        return (-180..180).collect();
    }
    let first = degree_cell(longitude - longitude_span);
    let last = degree_cell(longitude + longitude_span);
    (first..=last).map(wrap_column).collect()
}

/// Case d'une position.
fn cell_of(latitude: f64, longitude: f64) -> Cell {
    (degree_cell(latitude), wrap_column(degree_cell(longitude)))
}

/// Degré entier inférieur ou égal à la valeur.
fn degree_cell(degrees: f64) -> i32 {
    // Toujours entre -181 et 181 : la conversion est exacte.
    #[allow(clippy::cast_possible_truncation)]
    let cell = degrees.floor() as i32;
    cell
}

/// Ramène une colonne entre -180 et 179 (la colonne 180 est la colonne
/// -180, de l'autre côté du méridien).
fn wrap_column(column: i32) -> i32 {
    (column + 180).rem_euclid(COLUMNS) - 180
}

/// Distance à vol d'oiseau entre deux positions (formule de haversine).
fn distance_km(latitude_a: f64, longitude_a: f64, latitude_b: f64, longitude_b: f64) -> f64 {
    let (phi_a, phi_b) = (latitude_a.to_radians(), latitude_b.to_radians());
    let delta_phi = phi_b - phi_a;
    let delta_lambda = (longitude_b - longitude_a).to_radians();
    let h = (delta_phi / 2.0).sin().powi(2)
        + phi_a.cos() * phi_b.cos() * (delta_lambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}
