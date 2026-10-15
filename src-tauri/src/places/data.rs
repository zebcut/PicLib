//! Lecture de la base des lieux intégrée à l'application (fichiers texte de
//! `data/places/`, fabriqués par `scripts/build-places.sh`).

use std::collections::HashMap;

use super::{Area, Country, Place};

/// Textes des trois fichiers de la base.
pub(super) struct Texts {
    pub countries: &'static str,
    pub regions: &'static str,
    pub places: &'static str,
}

/// Base intégrée à l'application à la compilation.
pub(super) const EMBEDDED: Texts = Texts {
    countries: include_str!("../../data/places/countries.tsv"),
    regions: include_str!("../../data/places/regions.tsv"),
    places: include_str!("../../data/places/places.tsv"),
};

/// Base mal formée : erreur de fabrication.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid place data ({file}, line {line})")]
pub struct PlaceDataError {
    file: &'static str,
    line: usize,
}

/// Contenu de la base, avant indexation.
pub(super) struct Parsed {
    pub countries: Vec<Country>,
    pub areas: Vec<Area>,
    pub places: Vec<Place>,
}

/// Lit les trois fichiers. Les noms restent des tranches du texte intégré :
/// aucune copie.
pub(super) fn parse(texts: &Texts) -> Result<Parsed, PlaceDataError> {
    let mut countries = Vec::new();
    let mut country_positions = HashMap::new();
    for (line, columns) in lines(texts.countries) {
        let error = PlaceDataError {
            file: "countries.tsv",
            line,
        };
        let [code, french, english] = columns_of(columns).ok_or(error)?;
        country_positions.insert(code, countries.len());
        countries.push(Country { french, english });
    }

    let mut region_names = HashMap::new();
    for (line, columns) in lines(texts.regions) {
        let error = PlaceDataError {
            file: "regions.tsv",
            line,
        };
        let [country, region, name] = columns_of(columns).ok_or(error)?;
        region_names.insert((country, region), name);
    }

    let mut areas = Vec::new();
    let mut places = Vec::new();
    for (line, text) in lines(texts.places) {
        let error = PlaceDataError {
            file: "places.tsv",
            line,
        };
        if let Some(header) = text.strip_prefix('>') {
            // Début du groupe des lieux d'une région.
            let [country, region] = columns_of(header).ok_or(error)?;
            let country_position = *country_positions.get(country).ok_or(error)?;
            areas.push(Area {
                country: country_position,
                region: region_names.get(&(country, region)).copied(),
            });
        } else {
            let [latitude, longitude, name] = columns_of(text).ok_or(error)?;
            let area = areas.len().checked_sub(1).ok_or(error)?;
            places.push(Place {
                latitude: latitude.parse().map_err(|_| error)?,
                longitude: longitude.parse().map_err(|_| error)?,
                area,
                name,
            });
        }
    }
    Ok(Parsed {
        countries,
        areas,
        places,
    })
}

/// Lignes non vides d'un fichier, numérotées à partir de 1.
fn lines(text: &'static str) -> impl Iterator<Item = (usize, &'static str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, line)| (index + 1, line))
}

/// Exactement `N` colonnes séparées par des tabulations.
fn columns_of<const N: usize>(line: &'static str) -> Option<[&'static str; N]> {
    let columns: Vec<&'static str> = line.split('\t').collect();
    columns.try_into().ok()
}
