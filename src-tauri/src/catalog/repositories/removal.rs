//! Retrait de médias du catalogue, et leur remise en place (annulation).
//!
//! Les lignes retirées sont gardées telles quelles (toutes leurs colonnes),
//! avec l'état de leur vignette, leurs mots-clés, la trace du lieu rempli
//! depuis leur GPS, l'état de leur XMP et leurs liens de versions traitées :
//! l'annulation les remet exactement comme elles étaient, sous le même identifiant (jamais réattribué, voir la table
//! `id_watermarks`).
//!
//! Un dossier vidé par le retrait quitte aussi le catalogue ; l'annulation le remet.

use rusqlite::{Connection, params_from_iter, types::Value};

use super::folders;
use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders};
use crate::domain::{Folder, FolderId, MediaId};

/// Lignes d'une table, colonne par colonne.
#[derive(Debug, Clone, PartialEq, Default)]
struct StoredRows {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// Médias retirés, tels qu'ils étaient dans le catalogue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RemovedMedia {
    ids: Vec<MediaId>,
    media: StoredRows,
    thumbnails: StoredRows,
    place_lookups: StoredRows,
    media_keywords: StoredRows,
    collection_media: StoredRows,
    quick_collection: StoredRows,
    stack_media: StoredRows,
    xmp_sync: StoredRows,
    editor_sessions: StoredRows,
    /// Liens où le média est la version, l'original ou le fichier de
    /// travail.
    edited_versions: StoredRows,
    /// Dossiers des médias et leurs parents, à remettre s'ils ont été vidés.
    folders: Vec<Folder>,
}

impl RemovedMedia {
    /// Médias concernés (seulement ceux qui existaient).
    #[must_use]
    pub fn ids(&self) -> &[MediaId] {
        &self.ids
    }
}

/// Relève tout ce qu'il faut pour remettre les médias `ids` en place après
/// leur retrait (les identifiants inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn snapshot(connection: &Connection, ids: &[MediaId]) -> Result<RemovedMedia, CatalogError> {
    let media = read_rows(connection, "media", "id", ids)?;
    let found: Vec<MediaId> = media
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(Value::Integer(id)) => Some(MediaId::new(*id)),
            _ => None,
        })
        .collect();
    let thumbnails = read_rows(connection, "thumbnails", "media_id", &found)?;
    let place_lookups = read_rows(connection, "place_lookups", "media_id", &found)?;
    let media_keywords = read_rows(connection, "media_keywords", "media_id", &found)?;
    let collection_media = read_rows(connection, "collection_media", "media_id", &found)?;
    let quick_collection = read_rows(connection, "quick_collection", "media_id", &found)?;
    // La pile elle-même reste (voir la migration 27) : seules les places des
    // photos sont à remettre.
    let stack_media = read_rows(connection, "stack_media", "media_id", &found)?;
    let xmp_sync = read_rows(connection, "xmp_sync", "media_id", &found)?;
    let editor_sessions = read_rows(connection, "editor_sessions", "media_id", &found)?;
    let edited_versions = read_rows_any(
        connection,
        "edited_versions",
        &["media_id", "original_id", "working_id"],
        &found,
    )?;
    let folders = folders::lineage_of(connection, &folders_of(connection, &found)?)?;
    Ok(RemovedMedia {
        ids: found,
        media,
        thumbnails,
        place_lookups,
        media_keywords,
        collection_media,
        quick_collection,
        stack_media,
        xmp_sync,
        editor_sessions,
        edited_versions,
        folders,
    })
}

/// Retire les médias `ids` du catalogue (et, par cascade, leurs vignettes,
/// leurs mots-clés, la trace de leur lieu, l'état de leur XMP et leurs
/// liens de versions), puis les dossiers restés sans
/// média ; renvoie le nombre de médias retirés. À appeler dans une
/// transaction.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn delete(connection: &Connection, ids: &[MediaId]) -> Result<usize, CatalogError> {
    // Retient d'abord le plus grand identifiant attribué, pour qu'il ne soit
    // jamais redonné à un nouveau média.
    connection.execute(
        "UPDATE id_watermarks
         SET last_id = max(last_id, coalesce((SELECT max(id) FROM media), 0))
         WHERE name = 'media'",
        [],
    )?;
    let mut removed = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "DELETE FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        removed += connection
            .prepare_cached(&sql)?
            .execute(params_from_iter(chunk.iter().map(|id| id.get())))?;
    }
    folders::remove_without_media(connection)?;
    Ok(removed)
}

/// Remet en place des médias retirés, avec leurs vignettes, leurs mots-clés,
/// leurs collections, leur place dans une pile et la trace de leur lieu. À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible, ou si un média de même identifiant ou de
/// même nom dans le même dossier a été ajouté entre-temps.
pub fn restore(connection: &Connection, removed: &RemovedMedia) -> Result<(), CatalogError> {
    folders::restore_missing(connection, &removed.folders)?;
    insert_rows(connection, "media", &removed.media)?;
    insert_rows(connection, "thumbnails", &removed.thumbnails)?;
    insert_rows(connection, "place_lookups", &removed.place_lookups)?;
    insert_rows(connection, "media_keywords", &removed.media_keywords)?;
    insert_rows(connection, "collection_media", &removed.collection_media)?;
    insert_rows(connection, "quick_collection", &removed.quick_collection)?;
    insert_rows(connection, "stack_media", &removed.stack_media)?;
    // Les déclencheurs du suivi XMP ont recréé des lignes en remettant les
    // mots-clés : l'état gardé les remplace (pas de ligne : rien à écrire).
    for chunk in removed.ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "DELETE FROM xmp_sync WHERE media_id IN ({})",
            placeholders(1, chunk.len())
        );
        connection
            .prepare_cached(&sql)?
            .execute(params_from_iter(chunk.iter().map(|id| id.get())))?;
    }
    insert_rows(connection, "xmp_sync", &removed.xmp_sync)?;
    insert_rows(connection, "editor_sessions", &removed.editor_sessions)?;
    // Un lien resté en place (son fichier de travail seul retiré) retrouve
    // son état d'avant.
    replace_rows(connection, "edited_versions", &removed.edited_versions)?;
    Ok(())
}

/// Dossiers des médias `ids`, chacun une fois.
fn folders_of(connection: &Connection, ids: &[MediaId]) -> Result<Vec<FolderId>, CatalogError> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT DISTINCT folder_id FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                row.get::<_, i64>(0).map(FolderId::new)
            })?;
        for row in rows {
            let id = row?;
            if seen.insert(id) {
                found.push(id);
            }
        }
    }
    Ok(found)
}

fn read_rows(
    connection: &Connection,
    table: &str,
    id_column: &str,
    ids: &[MediaId],
) -> Result<StoredRows, CatalogError> {
    read_rows_any(connection, table, &[id_column], ids)
}

/// Lignes de `table` dont l'une des colonnes `id_columns` désigne un des
/// médias `ids`, chacune une fois.
fn read_rows_any(
    connection: &Connection,
    table: &str,
    id_columns: &[&str],
    ids: &[MediaId],
) -> Result<StoredRows, CatalogError> {
    let mut stored = StoredRows::default();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let list = placeholders(1, chunk.len());
        let condition = id_columns
            .iter()
            .map(|column| format!("{column} IN ({list})"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!("SELECT * FROM {table} WHERE {condition}");
        let mut statement = connection.prepare_cached(&sql)?;
        if stored.columns.is_empty() {
            stored.columns = statement
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
        }
        let width = stored.columns.len();
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                (0..width).map(|index| row.get::<_, Value>(index)).collect()
            })?;
        for row in rows {
            let row = row?;
            // Deux paquets peuvent ramener la même ligne (une colonne dans
            // chacun).
            if id_columns.len() == 1 || !stored.rows.contains(&row) {
                stored.rows.push(row);
            }
        }
    }
    Ok(stored)
}

fn insert_rows(
    connection: &Connection,
    table: &str,
    stored: &StoredRows,
) -> Result<(), CatalogError> {
    write_rows(connection, "INSERT", table, stored)
}

fn replace_rows(
    connection: &Connection,
    table: &str,
    stored: &StoredRows,
) -> Result<(), CatalogError> {
    write_rows(connection, "INSERT OR REPLACE", table, stored)
}

fn write_rows(
    connection: &Connection,
    verb: &str,
    table: &str,
    stored: &StoredRows,
) -> Result<(), CatalogError> {
    if stored.rows.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "{verb} INTO {table} ({}) VALUES ({})",
        stored.columns.join(", "),
        placeholders(1, stored.columns.len())
    );
    let mut statement = connection.prepare_cached(&sql)?;
    for row in &stored.rows {
        statement.execute(params_from_iter(row.iter()))?;
    }
    Ok(())
}
