//! Lieux remplis automatiquement depuis la position GPS (table
//! `place_lookups`, migration 8).

use rusqlite::{Connection, params};

use super::metadata::column;
use crate::catalog::CatalogError;
use crate::domain::{MediaId, MetadataField, PlaceNames};

/// Média géolocalisé dont le lieu n'a pas encore été cherché.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingPosition {
    /// Média.
    pub media_id: MediaId,
    /// Latitude en degrés décimaux.
    pub latitude: f64,
    /// Longitude en degrés décimaux.
    pub longitude: f64,
}

/// Médias géolocalisés jamais traités, dans l'ordre de leur identifiant.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn pending(connection: &Connection) -> Result<Vec<PendingPosition>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT media.id, media.gps_latitude, media.gps_longitude
         FROM media LEFT JOIN place_lookups ON place_lookups.media_id = media.id
         WHERE media.gps_latitude IS NOT NULL AND media.gps_longitude IS NOT NULL
               AND place_lookups.media_id IS NULL
         ORDER BY media.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(PendingPosition {
            media_id: MediaId::new(row.get(0)?),
            latitude: row.get(1)?,
            longitude: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Remplit les champs Pays, Région et Ville **vides** du média avec `names`
/// (`None` : aucun lieu trouvé), et le note comme traité. Un champ déjà
/// rempli n'est jamais modifié, même par l'utilisateur à l'instant : la
/// condition est vérifiée par la base au moment d'écrire. Renvoie vrai si au
/// moins un champ a été rempli. Un média qui a quitté le catalogue entre-temps
/// est ignoré. À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn fill(
    connection: &Connection,
    media_id: MediaId,
    names: Option<&PlaceNames>,
) -> Result<bool, CatalogError> {
    let country = fill_empty(
        connection,
        MetadataField::Country,
        media_id,
        names.map(|names| names.country.as_str()),
    )?;
    let region = fill_empty(
        connection,
        MetadataField::Region,
        media_id,
        names.and_then(|names| names.region.as_deref()),
    )?;
    let city = fill_empty(
        connection,
        MetadataField::City,
        media_id,
        names.map(|names| names.city.as_str()),
    )?;
    // « WHERE EXISTS » : un média supprimé entre-temps n'est pas noté (la
    // clé étrangère le refuserait).
    connection
        .prepare_cached(
            "INSERT INTO place_lookups (media_id, country, region, city)
             SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)
             ON CONFLICT (media_id) DO NOTHING",
        )?
        .execute(params![media_id.get(), country, region, city])?;
    Ok(country.is_some() || region.is_some() || city.is_some())
}

/// Donne la valeur au champ s'il est vide ; renvoie la valeur si elle a été
/// écrite.
fn fill_empty<'v>(
    connection: &Connection,
    field: MetadataField,
    media_id: MediaId,
    value: Option<&'v str>,
) -> Result<Option<&'v str>, CatalogError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let sql = format!(
        "UPDATE media SET {column} = ?1 WHERE id = ?2 AND {column} IS NULL",
        column = column(field)
    );
    let changed = connection
        .prepare_cached(&sql)?
        .execute(params![value, media_id.get()])?;
    Ok((changed > 0).then_some(value))
}
