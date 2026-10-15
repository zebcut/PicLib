//! Rotation choisie par l'utilisateur, pour un ensemble de médias.

use rusqlite::{Connection, params_from_iter, types::Value};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders, rotation_from_sql};
use crate::domain::{MediaId, Rotation, RotationDirection};

/// Rotation actuelle de chaque média existant parmi `ids` (les identifiants
/// inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn read(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, Rotation)>, CatalogError> {
    let mut rotations = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id, rotation FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
        for row in rows {
            let (id, degrees) = row?;
            rotations.push((MediaId::new(id), rotation_from_sql(degrees)?));
        }
    }
    Ok(rotations)
}

/// Fait pivoter les médias `ids` d'un quart de tour ; renvoie le nombre de
/// médias modifiés. À appeler dans une transaction pour un ensemble
/// cohérent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn rotate(
    connection: &Connection,
    ids: &[MediaId],
    direction: RotationDirection,
) -> Result<usize, CatalogError> {
    let degrees = i64::from(direction.clockwise_degrees());
    let mut changed = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "UPDATE media SET rotation = (rotation + ?1) % 360 WHERE id IN ({})",
            placeholders(2, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = std::iter::once(Value::Integer(degrees))
            .chain(chunk.iter().map(|id| Value::Integer(id.get())));
        changed += statement.execute(params_from_iter(parameters))?;
    }
    Ok(changed)
}

/// Donne la rotation `rotation` aux médias `ids` ; renvoie le nombre de
/// médias modifiés. À appeler dans une transaction pour un ensemble
/// cohérent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn set(
    connection: &Connection,
    ids: &[MediaId],
    rotation: Rotation,
) -> Result<usize, CatalogError> {
    let mut changed = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "UPDATE media SET rotation = ?1 WHERE id IN ({})",
            placeholders(2, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = std::iter::once(Value::Integer(i64::from(rotation.degrees())))
            .chain(chunk.iter().map(|id| Value::Integer(id.get())));
        changed += statement.execute(params_from_iter(parameters))?;
    }
    Ok(changed)
}
