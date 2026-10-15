//! Photos géolocalisées pour la vue Carte : celles
//! de la source affichée et de ses filtres, regroupées par zone dans la
//! partie visible de la carte. Le regroupement se fait dans SQLite : la
//! carte ne reçoit que quelques centaines de groupes, même avec 50 000
//! photos.

use rusqlite::{Connection, ToSql};

use super::library_query::LibraryQuery;
use crate::catalog::CatalogError;
use crate::catalog::sql::size_from_sql;
use crate::domain::MediaId;

/// Nombre maximal de groupes renvoyés pour une vue.
const MAX_CLUSTERS: i64 = 2000;

/// Zone de la carte, en degrés. `west` peut dépasser `east` quand la zone
/// passe l'antiméridien (le Pacifique au milieu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoArea {
    /// Latitude du bord sud.
    pub south: f64,
    /// Longitude du bord ouest.
    pub west: f64,
    /// Latitude du bord nord.
    pub north: f64,
    /// Longitude du bord est.
    pub east: f64,
}

// Une zone n'a jamais de NaN (voir `GeoArea::new`) : l'égalité est totale.
impl Eq for GeoArea {}

impl GeoArea {
    /// Zone ramenée aux bornes du monde ; `None` si elle est vide ou fausse.
    #[must_use]
    pub fn new(south: f64, west: f64, north: f64, east: f64) -> Option<Self> {
        let values = [south, west, north, east];
        if values.iter().any(|value| !value.is_finite()) || south > north {
            return None;
        }
        // Une carte qui fait plusieurs fois le tour du monde le couvre.
        let (west, east) = if east - west >= 360.0 {
            (-180.0, 180.0)
        } else {
            (wrap_longitude(west), wrap_longitude(east))
        };
        Some(Self {
            south: south.clamp(-90.0, 90.0),
            west,
            north: north.clamp(-90.0, 90.0),
            east,
        })
    }

    /// Condition SQL : la position de `media` est dans la zone. Faite de
    /// nombres seulement (aucun texte venu de l'interface).
    #[must_use]
    pub fn condition(&self) -> String {
        let longitude = if self.west <= self.east {
            format!(
                "media.gps_longitude BETWEEN {} AND {}",
                self.west, self.east
            )
        } else {
            format!(
                "(media.gps_longitude >= {} OR media.gps_longitude <= {})",
                self.west, self.east
            )
        };
        format!(
            "media.gps_latitude BETWEEN {} AND {} AND {longitude}",
            self.south, self.north
        )
    }
}

/// Longitude ramenée entre -180 et 180.
fn wrap_longitude(longitude: f64) -> f64 {
    if (-180.0..=180.0).contains(&longitude) {
        longitude
    } else {
        (longitude + 180.0).rem_euclid(360.0) - 180.0
    }
}

/// Groupe de photos proches.
#[derive(Debug, Clone, PartialEq)]
pub struct MapCluster {
    /// Centre du groupe (moyenne des positions).
    pub latitude: f64,
    /// Centre du groupe.
    pub longitude: f64,
    /// Nombre de photos.
    pub count: u64,
    /// Une photo du groupe, pour sa vignette.
    pub cover: MediaId,
    /// Zone couverte par les photos du groupe.
    pub area: GeoArea,
}

/// Photos géolocalisées de `query` dans `area`, regroupées dans des cases
/// de `cell_degrees` degrés de côté.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn clusters(
    connection: &Connection,
    query: &LibraryQuery,
    area: GeoArea,
    cell_degrees: f64,
) -> Result<Vec<MapCluster>, CatalogError> {
    let sql = query.grid_sql(connection)?;
    let cell = cell_degrees.clamp(1e-6, 360.0);
    let cell_param: &dyn ToSql = &cell;
    let limit_param: &dyn ToSql = &MAX_CLUSTERS;
    // Les positions sont décalées pour être positives : la conversion en
    // entier arrondit alors vers le bas.
    let mut statement = connection.prepare_cached(&format!(
        "SELECT avg(media.gps_latitude), avg(media.gps_longitude), count(*), max(media.id),
            min(media.gps_latitude), min(media.gps_longitude),
            max(media.gps_latitude), max(media.gps_longitude)
         FROM media
         WHERE ({}) AND media.gps_latitude IS NOT NULL AND media.gps_longitude IS NOT NULL
            AND {}
         GROUP BY CAST((media.gps_latitude + 90.0) / :cell AS INTEGER),
            CAST((media.gps_longitude + 180.0) / :cell AS INTEGER)
         LIMIT :limit",
        sql.condition,
        area.condition()
    ))?;
    let rows = statement
        .query_map(
            sql.params(&[(":cell", cell_param), (":limit", limit_param)])
                .as_slice(),
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    [
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, f64>(6)?,
                        row.get::<_, f64>(7)?,
                    ],
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(latitude, longitude, count, cover, [south, west, north, east])| {
                Ok(MapCluster {
                    latitude,
                    longitude,
                    count: size_from_sql(count)?,
                    cover: MediaId::new(cover),
                    area: GeoArea {
                        south,
                        west,
                        north,
                        east,
                    },
                })
            },
        )
        .collect()
}

/// Photos géolocalisées de `query` dans `area`, au plus `limit`, dans
/// l'ordre de la grille (sélectionner les photos d'un groupe).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn ids_in(
    connection: &Connection,
    query: &LibraryQuery,
    area: GeoArea,
    limit: u64,
) -> Result<Vec<MediaId>, CatalogError> {
    let sql = query.grid_sql(connection)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let limit_param: &dyn ToSql = &limit;
    let ids = connection
        .prepare_cached(&format!(
            "SELECT media.id FROM media WHERE ({}) AND {} {} LIMIT :limit",
            sql.condition,
            area.condition(),
            sql.order
        ))?
        .query_map(sql.params(&[(":limit", limit_param)]).as_slice(), |row| {
            row.get::<_, i64>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().map(MediaId::new).collect())
}

/// Parmi les médias `ids`, combien existent, et combien ont une position,
/// un pays, une région ou une ville (que les placer sur la carte
/// remplacerait).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn described_among(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<(u64, u64), CatalogError> {
    let values: Vec<rusqlite::types::Value> = ids
        .iter()
        .map(|id| rusqlite::types::Value::from(id.get()))
        .collect();
    let (total, described): (i64, i64) = connection
        .prepare_cached(
            "SELECT count(*), count(coalesce(gps_latitude, country, region, city))
             FROM media WHERE id IN rarray(?1)",
        )?
        .query_row([std::rc::Rc::new(values)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    Ok((size_from_sql(total)?, size_from_sql(described)?))
}

/// Nombre de photos de `query` avec et sans position.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn located_counts(
    connection: &Connection,
    query: &LibraryQuery,
) -> Result<(u64, u64), CatalogError> {
    let sql = query.grid_sql(connection)?;
    let (located, total): (i64, i64) = connection
        .prepare_cached(&format!(
            "SELECT count(media.gps_latitude), count(*) FROM media WHERE {}",
            sql.condition
        ))?
        .query_row(sql.params(&[]).as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    Ok((size_from_sql(located)?, size_from_sql(total - located)?))
}
