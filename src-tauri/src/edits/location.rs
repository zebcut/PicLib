//! Position GPS posée ou retirée à la main : en
//! glissant des photos sur la carte, ou par « Retirer la position ».
//! Annulable : chaque photo retrouve sa position et son lieu d'avant.
//!
//! Une photo placée sur la carte prend la position du point choisi et le
//! lieu trouvé pour lui : pays, région et ville sont remplacés, quels qu'ils
//! soient (l'interface demande confirmation avant d'écraser), ainsi que le
//! mot-clé de lieu. Le lieu précis est gardé. Retirer la position garde le
//! lieu.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::catalog::repositories::{media_details, metadata};
use crate::domain::{GpsPosition, MediaId, MetadataField, PlaceNames};
use crate::keywords::places::{self, PlaceLinks};

/// Champs du lieu remplacés, dans l'ordre des colonnes de `place_lookups`.
const PLACE_FIELDS: [MetadataField; 3] = [
    MetadataField::Country,
    MetadataField::Region,
    MetadataField::City,
];

/// Valeurs des trois champs du lieu, dans l'ordre de `PLACE_FIELDS`.
type Place = [Option<String>; 3];

/// Position posée à la main, avec le lieu trouvé pour elle.
#[derive(Debug, Clone, PartialEq)]
pub struct NewLocation {
    /// Position du point choisi.
    pub position: GpsPosition,
    /// Lieu le plus proche ; `None` : aucun (en pleine mer...), les champs
    /// du lieu sont alors vidés.
    pub place: Option<PlaceNames>,
}

/// Nouvelle position de plusieurs médias.
#[derive(Debug, Clone, PartialEq)]
pub struct LocationEdit {
    /// Position donnée (`None` : retirée).
    after: Option<GpsPosition>,
    /// Lieu écrit avec la position (sans objet quand elle est retirée).
    place: Place,
    /// Médias qui changent, avec leur état d'avant.
    before: Vec<Before>,
    /// Mots-clés de lieu à remplacer.
    links: PlaceLinks,
}

/// État d'un média avant le changement.
#[derive(Debug, Clone, PartialEq)]
struct Before {
    id: MediaId,
    position: Option<GpsPosition>,
    /// Champs du lieu.
    place: Place,
    /// Trace de la recherche du lieu (`place_lookups`) ; `None` : lieu
    /// jamais cherché.
    lookup: Option<Place>,
}

impl LocationEdit {
    /// Prépare le changement : lit l'état actuel des médias `ids` (les
    /// identifiants inconnus sont ignorés) ; ceux qui ont déjà cette
    /// position et ce lieu sont laissés de côté.
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible, ou ne peut pas être écrite (mots-clés
    /// du nouveau lieu, créés s'il le faut).
    pub(super) fn new(
        connection: &Connection,
        after: Option<NewLocation>,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let position = after.as_ref().map(|location| location.position);
        let place = after
            .and_then(|location| location.place)
            .map_or([None, None, None], place_of);
        let mut before = Vec::with_capacity(ids.len());
        for (id, current) in read_positions(connection, ids)? {
            let current_place = read_place(connection, id)?;
            let unchanged = current == position && (position.is_none() || current_place == place);
            if unchanged {
                continue;
            }
            before.push(Before {
                id,
                position: current,
                place: current_place,
                lookup: read_lookup(connection, id)?,
            });
        }
        let mut edit = Self {
            after: position,
            place,
            before,
            links: PlaceLinks::default(),
        };
        if edit.after.is_some() {
            edit.links = edit.place_links(connection)?;
        }
        Ok(edit)
    }

    /// Mots-clés de lieu à remplacer pour le nouveau lieu.
    fn place_links(&self, connection: &Connection) -> Result<PlaceLinks, CatalogError> {
        let before = places::read(connection, &self.media_ids())?;
        let after: HashMap<_, _> = before
            .iter()
            .map(|(id, fields)| {
                let fields = PLACE_FIELDS
                    .into_iter()
                    .zip(&self.place)
                    .fold(fields.clone(), |fields, (field, value)| {
                        fields.with(field, value.as_deref())
                    });
                (*id, fields)
            })
            .collect();
        places::changes(connection, &before, &after)
    }

    /// Médias dont la position change.
    #[must_use]
    pub fn media_ids(&self) -> Vec<MediaId> {
        self.before.iter().map(|media| media.id).collect()
    }

    /// Vrai si la position est retirée.
    #[must_use]
    pub fn is_removal(&self) -> bool {
        self.after.is_none()
    }

    pub(super) fn changes_nothing(&self) -> bool {
        self.before.is_empty()
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for media in &self.before {
            media_details::set_gps(connection, media.id, self.after)?;
            if self.after.is_some() {
                write_place(connection, media.id, &self.place)?;
                // Lieu écrit par PicLib : la recherche de fond ne le refait pas.
                write_lookup(connection, media.id, Some(&self.place))?;
            }
        }
        self.links.apply(connection)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.links.revert(connection)?;
        for media in &self.before {
            media_details::set_gps(connection, media.id, media.position)?;
            if self.after.is_some() {
                write_place(connection, media.id, &media.place)?;
                write_lookup(connection, media.id, media.lookup.as_ref())?;
            }
        }
        Ok(())
    }
}

/// Champs du lieu trouvé.
fn place_of(names: PlaceNames) -> Place {
    [Some(names.country), names.region, Some(names.city)]
}

/// Positions actuelles des médias `ids` qui existent.
fn read_positions(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, Option<GpsPosition>)>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT gps_latitude, gps_longitude, gps_altitude FROM media WHERE id = ?1",
    )?;
    let mut positions = Vec::with_capacity(ids.len());
    for &id in ids {
        let mut rows = statement.query([id.get()])?;
        let Some(row) = rows.next()? else {
            continue;
        };
        let latitude: Option<f64> = row.get(0)?;
        let longitude: Option<f64> = row.get(1)?;
        let altitude: Option<f64> = row.get(2)?;
        positions.push((
            id,
            latitude
                .zip(longitude)
                .map(|(latitude, longitude)| GpsPosition {
                    latitude,
                    longitude,
                    altitude,
                }),
        ));
    }
    Ok(positions)
}

/// Pays, région et ville du média.
fn read_place(connection: &Connection, id: MediaId) -> Result<Place, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT country, region, city FROM media WHERE id = ?1")?
        .query_row([id.get()], |row| {
            Ok([row.get(0)?, row.get(1)?, row.get(2)?])
        })?)
}

/// Écrit le pays, la région et la ville du média.
fn write_place(connection: &Connection, id: MediaId, place: &Place) -> Result<(), CatalogError> {
    for (field, value) in PLACE_FIELDS.into_iter().zip(place) {
        metadata::write(connection, field, value.as_deref(), &[id])?;
    }
    Ok(())
}

/// Ce que la recherche du lieu a écrit pour le média ; `None` : jamais
/// cherché.
fn read_lookup(connection: &Connection, id: MediaId) -> Result<Option<Place>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT country, region, city FROM place_lookups WHERE media_id = ?1")?
        .query_row([id.get()], |row| {
            Ok([row.get(0)?, row.get(1)?, row.get(2)?])
        })
        .optional()?)
}

/// Remplace la trace de la recherche du lieu (`None` : à chercher).
fn write_lookup(
    connection: &Connection,
    id: MediaId,
    lookup: Option<&Place>,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM place_lookups WHERE media_id = ?1")?
        .execute([id.get()])?;
    if let Some([country, region, city]) = lookup {
        connection
            .prepare_cached(
                "INSERT INTO place_lookups (media_id, country, region, city)
                 VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![id.get(), country, region, city])?;
    }
    Ok(())
}
