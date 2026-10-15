//! Lecture du journal des opérations sur les fichiers, pour le consulter : les opérations récentes, et les
//! fichiers de chacune. Les valeurs sont rendues telles qu'écrites
//! (`kind`, `status`, `result`) : l'interface les traduit.

use rusqlite::{Connection, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::size_from_sql;

/// Opération du journal, avec le décompte de ses fichiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRecord {
    /// Identifiant.
    pub id: i64,
    /// Nature : `trash`, `copy` ou `move`.
    pub kind: String,
    /// Début, en millisecondes depuis 1970 (UTC).
    pub started_at: i64,
    /// Fin, s'il y en a une.
    pub finished_at: Option<i64>,
    /// État : `running`, `completed`, `cancelled` ou `failed`.
    pub status: String,
    /// Fichiers traités.
    pub done: u64,
    /// Fichiers introuvables.
    pub missing: u64,
    /// Fichiers en échec.
    pub failed: u64,
    /// Fichiers restés en attente.
    pub pending: u64,
}

/// Fichier d'une opération.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRecord {
    /// Chemin d'origine.
    pub source: String,
    /// Chemin d'arrivée, s'il est connu.
    pub destination: Option<String>,
    /// Résultat : `pending`, `done`, `missing` ou `failed`.
    pub result: String,
    /// Raison d'un échec.
    pub detail: Option<String>,
}

/// Les `limit` dernières opérations, la plus récente d'abord.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn recent(connection: &Connection, limit: u32) -> Result<Vec<OperationRecord>, CatalogError> {
    let rows = connection
        .prepare_cached(
            "SELECT file_operations.id, kind, started_at, finished_at, status,
                count(CASE result WHEN 'done' THEN 1 END),
                count(CASE result WHEN 'missing' THEN 1 END),
                count(CASE result WHEN 'failed' THEN 1 END),
                count(CASE result WHEN 'pending' THEN 1 END)
             FROM file_operations
             LEFT JOIN file_operation_items
                 ON file_operation_items.operation_id = file_operations.id
             GROUP BY file_operations.id
             ORDER BY file_operations.id DESC
             LIMIT ?1",
        )?
        .query_map([limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, String>(4)?,
                [
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ],
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, kind, started_at, finished_at, status, counts)| {
            Ok(OperationRecord {
                id,
                kind,
                started_at,
                finished_at,
                status,
                done: size_from_sql(counts[0])?,
                missing: size_from_sql(counts[1])?,
                failed: size_from_sql(counts[2])?,
                pending: size_from_sql(counts[3])?,
            })
        })
        .collect()
}

/// Les `limit` premiers fichiers de l'opération `id`, dans l'ordre.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn items(
    connection: &Connection,
    id: i64,
    limit: u32,
) -> Result<Vec<ItemRecord>, CatalogError> {
    Ok(connection
        .prepare_cached(
            "SELECT source_path, destination_path, result, detail FROM file_operation_items
             WHERE operation_id = ?1 ORDER BY id LIMIT ?2",
        )?
        .query_map(params![id, limit], |row| {
            Ok(ItemRecord {
                source: row.get(0)?,
                destination: row.get(1)?,
                result: row.get(2)?,
                detail: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}
