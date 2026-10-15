//! Sessions d'import.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, named_params, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{
    import_mode_from_text, import_mode_to_text, import_status_from_text, import_status_to_text,
    path_to_text, size_from_sql, size_to_sql, text_to_path,
};
use crate::domain::{Import, ImportCounts, ImportId, ImportMode, ImportStatus, Timestamp};

/// Enregistre le début d'un import (état « en cours ») et renvoie son
/// identifiant.
///
/// # Errors
///
/// Échoue si la base est illisible ou si le chemin n'est pas en UTF-8.
pub fn start(
    connection: &Connection,
    mode: ImportMode,
    source_path: &Path,
    started_at: Timestamp,
) -> Result<ImportId, CatalogError> {
    connection.execute(
        "INSERT INTO imports (mode, source_path, started_at, status) VALUES (?1, ?2, ?3, ?4)",
        params![
            import_mode_to_text(mode),
            path_to_text(source_path)?,
            started_at.unix_millis(),
            import_status_to_text(ImportStatus::Running),
        ],
    )?;
    Ok(ImportId::new(connection.last_insert_rowid()))
}

/// Note le dossier où un import par copie range les fichiers.
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base est illisible.
pub fn set_destination(
    connection: &Connection,
    id: ImportId,
    destination: &Path,
) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE imports SET destination_path = ?2 WHERE id = ?1",
        params![id.get(), path_to_text(destination)?],
    )?;
    Ok(())
}

/// Enregistre la fin d'un import : son état final et ses compteurs.
///
/// # Errors
///
/// Échoue si l'import n'existe pas ou si la base est illisible.
pub fn finish(
    connection: &Connection,
    id: ImportId,
    status: ImportStatus,
    counts: &ImportCounts,
    finished_at: Timestamp,
) -> Result<(), CatalogError> {
    let updated = connection.execute(
        "UPDATE imports SET finished_at = :finished_at, status = :status,
            added_photos = :added_photos, added_videos = :added_videos,
            already_in_catalog = :already_in_catalog, duplicates_skipped = :duplicates_skipped,
            without_exif_date = :without_exif_date, unreadable_metadata = :unreadable_metadata,
            with_recovered_keywords = :with_recovered_keywords,
            metadata_conflicts = :metadata_conflicts, copy_failures = :copy_failures
         WHERE id = :id",
        named_params! {
            ":finished_at": finished_at.unix_millis(),
            ":status": import_status_to_text(status),
            ":added_photos": size_to_sql(counts.added_photos)?,
            ":added_videos": size_to_sql(counts.added_videos)?,
            ":already_in_catalog": size_to_sql(counts.already_in_catalog)?,
            ":duplicates_skipped": size_to_sql(counts.duplicates_skipped)?,
            ":without_exif_date": size_to_sql(counts.without_exif_date)?,
            ":unreadable_metadata": size_to_sql(counts.unreadable_metadata)?,
            ":with_recovered_keywords": size_to_sql(counts.with_recovered_keywords)?,
            ":metadata_conflicts": size_to_sql(counts.metadata_conflicts)?,
            ":copy_failures": size_to_sql(counts.copy_failures)?,
            ":id": id.get(),
        },
    )?;
    if updated == 0 {
        return Err(CatalogError::NotFound {
            entity: "import",
            id: id.get(),
        });
    }
    Ok(())
}

/// Imports restés « en cours ».
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn running(connection: &Connection) -> Result<Vec<ImportId>, CatalogError> {
    let mut statement =
        connection.prepare("SELECT id FROM imports WHERE status = 'running' ORDER BY id")?;
    let ids = statement
        .query_map([], |row| row.get::<_, i64>(0).map(ImportId::new))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Clôt comme « en échec » un import resté « en cours » : l'application
/// s'est arrêtée pendant qu'il se faisait. Ses nombres de photos et de
/// vidéos ajoutées sont recomptés au catalogue (ce qui a été écrit l'est).
/// Sans effet sur un import terminé.
///
/// # Errors
///
/// Échoue si la base est illisible ou ne peut pas être écrite.
pub fn close_interrupted(
    connection: &Connection,
    id: ImportId,
    finished_at: Timestamp,
) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE imports SET status = 'failed', finished_at = ?1,
            added_photos = (SELECT count(*) FROM media
                            WHERE media.import_id = imports.id AND media.kind = 'photo'),
            added_videos = (SELECT count(*) FROM media
                            WHERE media.import_id = imports.id AND media.kind = 'video')
         WHERE id = ?2 AND status = 'running'",
        params![finished_at.unix_millis(), id.get()],
    )?;
    Ok(())
}

/// Cherche un import par son identifiant.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn find(connection: &Connection, id: ImportId) -> Result<Option<Import>, CatalogError> {
    let row = connection
        .query_row(
            "SELECT mode, source_path, started_at, finished_at, status, added_photos, added_videos,
                already_in_catalog, duplicates_skipped, without_exif_date, unreadable_metadata,
                with_recovered_keywords, metadata_conflicts, copy_failures, destination_path
             FROM imports WHERE id = ?1",
            [id.get()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, String>(4)?,
                    [
                        row.get::<_, i64>(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                    ],
                    row.get::<_, Option<String>>(14)?,
                ))
            },
        )
        .optional()?;

    row.map(
        |(mode, source_path, started_at, finished_at, status, counts, destination_path)| {
            let [
                photos,
                videos,
                existing,
                duplicates,
                without_date,
                unreadable,
                with_keywords,
                conflicts,
                copy_failures,
            ] = counts;
            Ok(Import {
                id,
                mode: import_mode_from_text(&mode)?,
                source_path: text_to_path(source_path),
                destination_path: destination_path.map(text_to_path),
                started_at: Timestamp::from_unix_millis(started_at),
                finished_at: finished_at.map(Timestamp::from_unix_millis),
                status: import_status_from_text(&status)?,
                counts: ImportCounts {
                    added_photos: size_from_sql(photos)?,
                    added_videos: size_from_sql(videos)?,
                    already_in_catalog: size_from_sql(existing)?,
                    duplicates_skipped: size_from_sql(duplicates)?,
                    without_exif_date: size_from_sql(without_date)?,
                    unreadable_metadata: size_from_sql(unreadable)?,
                    with_recovered_keywords: size_from_sql(with_keywords)?,
                    metadata_conflicts: size_from_sql(conflicts)?,
                    copy_failures: size_from_sql(copy_failures)?,
                },
            })
        },
    )
    .transpose()
}
