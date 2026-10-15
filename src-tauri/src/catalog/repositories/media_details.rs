//! Informations techniques et position GPS complétées après l'import, en
//! relisant les fichiers(« Lire les métadonnées des fichiers ») :
//! seules les valeurs manquantes sont écrites, et l'annulation les efface.

use rusqlite::{Connection, named_params, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::size_to_sql;
use crate::domain::{GpsPosition, MediaDetails, MediaId, Orientation};

/// Complète les informations techniques du média `id` avec les valeurs
/// présentes dans `details`, sans jamais remplacer une valeur connue (la
/// position GPS n'est pas concernée : voir [`set_gps`]). À appeler dans une
/// transaction.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn fill(
    connection: &Connection,
    id: MediaId,
    details: &MediaDetails,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "UPDATE media SET
            width = coalesce(width, :width),
            height = coalesce(height, :height),
            orientation = coalesce(orientation, :orientation),
            camera_make = coalesce(camera_make, :camera_make),
            camera_model = coalesce(camera_model, :camera_model),
            lens = coalesce(lens, :lens),
            focal_length = coalesce(focal_length, :focal_length),
            aperture = coalesce(aperture, :aperture),
            exposure_time = coalesce(exposure_time, :exposure_time),
            iso = coalesce(iso, :iso),
            flash_fired = coalesce(flash_fired, :flash_fired),
            duration_ms = coalesce(duration_ms, :duration_ms)
         WHERE id = :id",
    )?;
    execute(&mut statement, id, details)
}

/// Efface les informations techniques du média `id` présentes dans
/// `details` (annulation de [`fill`]) ; les autres ne changent pas. À
/// appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn clear(
    connection: &Connection,
    id: MediaId,
    details: &MediaDetails,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "UPDATE media SET
            width = CASE WHEN :width IS NULL THEN width END,
            height = CASE WHEN :height IS NULL THEN height END,
            orientation = CASE WHEN :orientation IS NULL THEN orientation END,
            camera_make = CASE WHEN :camera_make IS NULL THEN camera_make END,
            camera_model = CASE WHEN :camera_model IS NULL THEN camera_model END,
            lens = CASE WHEN :lens IS NULL THEN lens END,
            focal_length = CASE WHEN :focal_length IS NULL THEN focal_length END,
            aperture = CASE WHEN :aperture IS NULL THEN aperture END,
            exposure_time = CASE WHEN :exposure_time IS NULL THEN exposure_time END,
            iso = CASE WHEN :iso IS NULL THEN iso END,
            flash_fired = CASE WHEN :flash_fired IS NULL THEN flash_fired END,
            duration_ms = CASE WHEN :duration_ms IS NULL THEN duration_ms END
         WHERE id = :id",
    )?;
    execute(&mut statement, id, details)
}

fn execute(
    statement: &mut rusqlite::CachedStatement<'_>,
    id: MediaId,
    details: &MediaDetails,
) -> Result<(), CatalogError> {
    statement.execute(named_params! {
        ":id": id.get(),
        ":width": details.width,
        ":height": details.height,
        ":orientation": details.orientation.map(Orientation::get),
        ":camera_make": details.camera_make,
        ":camera_model": details.camera_model,
        ":lens": details.lens,
        ":focal_length": details.focal_length_mm,
        ":aperture": details.aperture,
        ":exposure_time": details.exposure_time_s,
        ":iso": details.iso,
        ":flash_fired": details.flash_fired,
        ":duration_ms": details.duration_ms.map(size_to_sql).transpose()?,
    })?;
    Ok(())
}

/// Donne la position `gps` au média `id` (`None` : aucune). À appeler dans
/// une transaction.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_gps(
    connection: &Connection,
    id: MediaId,
    gps: Option<GpsPosition>,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE media SET gps_latitude = ?1, gps_longitude = ?2, gps_altitude = ?3
             WHERE id = ?4",
        )?
        .execute(params![
            gps.map(|position| position.latitude),
            gps.map(|position| position.longitude),
            gps.and_then(|position| position.altitude),
            id.get(),
        ])?;
    Ok(())
}
