//! Journal des opérations sur les fichiers : chaque opération
//! (mise à la corbeille, copie, déplacement de dossier) et le résultat pour chaque fichier.
//!
//! Journal d'intention : un fichier est noté « en attente »
//! avant l'opération sur le disque, puis « fait » dans la même transaction
//! que son inscription au catalogue (ou son retrait). Les lignes restées en
//! attente après un arrêt brutal sont terminées à l'ouverture suivante.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{path_to_text, text_to_path};
use crate::domain::{ImportId, Timestamp};

/// Identifiant d'une opération du journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationId(i64);

/// Identifiant d'un fichier d'une opération du journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemId(i64);

/// Opération restée « en cours » : l'application s'est arrêtée pendant
/// qu'elle se faisait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptedOperation {
    /// Opération.
    pub id: OperationId,
    /// Nature.
    pub kind: OperationKind,
    /// Import dont elle fait partie (copie).
    pub import_id: Option<ImportId>,
}

/// Fichier resté « en attente » dans une opération interrompue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingItem {
    /// Ligne du journal.
    pub id: ItemId,
    /// Fichier d'origine.
    pub source: PathBuf,
    /// Copie : son nom définitif si elle est vérifiée, sinon le dossier où
    /// elle s'écrivait.
    pub destination: Option<PathBuf>,
    /// Copie vérifiée : son empreinte.
    pub content_hash: Option<String>,
}

/// Nature d'une opération.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// Mise à la corbeille.
    Trash,
    /// Copie vérifiée (import par copie).
    Copy,
    /// Déplacement d'un dossier(panneau Dossiers).
    Move,
    /// Copie de sauvegarde des originaux, faite pendant un import par
    /// copie ; jamais inscrite au catalogue.
    Backup,
}

/// État final d'une opération.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    /// Terminée (des fichiers ont pu échouer : voir leur résultat).
    Completed,
    /// Arrêtée à la demande de l'utilisateur.
    Cancelled,
    /// Interrompue par une erreur.
    Failed,
}

/// Résultat pour un fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemResult {
    /// Fait.
    Done,
    /// Fichier introuvable : rien n'a été fait.
    Missing,
    /// Échec (voir le détail).
    Failed,
}

/// Commence une opération, rattachée à un import s'il y a lieu ; à
/// terminer par [`finish`].
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn start(
    connection: &Connection,
    kind: OperationKind,
    import: Option<ImportId>,
    started_at: Timestamp,
) -> Result<OperationId, CatalogError> {
    connection.execute(
        "INSERT INTO file_operations (kind, import_id, started_at, status)
         VALUES (?1, ?2, ?3, 'running')",
        params![
            kind_text(kind),
            import.map(ImportId::get),
            started_at.unix_millis()
        ],
    )?;
    Ok(OperationId(connection.last_insert_rowid()))
}

/// Note un fichier « en attente », avant l'opération sur le disque.
/// `destination` : pour une copie, le dossier où elle va s'écrire.
///
/// # Errors
///
/// Échoue si un chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn record_pending(
    connection: &Connection,
    operation: OperationId,
    source: &Path,
    destination: Option<&Path>,
) -> Result<ItemId, CatalogError> {
    let destination = destination.map(path_to_text).transpose()?;
    connection
        .prepare_cached(
            "INSERT INTO file_operation_items (operation_id, source_path, destination_path, result)
             VALUES (?1, ?2, ?3, 'pending')",
        )?
        .execute(params![operation.0, path_to_text(source)?, destination])?;
    Ok(ItemId(connection.last_insert_rowid()))
}

/// Copie vérifiée, sur le point de prendre son nom définitif : il est noté
/// avec son empreinte, pour la retrouver après un arrêt brutal.
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn set_verified(
    connection: &Connection,
    item: ItemId,
    destination: &Path,
    content_hash: &str,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE file_operation_items SET destination_path = ?2, content_hash = ?3
             WHERE id = ?1",
        )?
        .execute(params![item.0, path_to_text(destination)?, content_hash])?;
    Ok(())
}

/// Note le résultat d'un fichier « en attente ».
///
/// # Errors
///
/// Échoue si le chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn complete(
    connection: &Connection,
    item: ItemId,
    destination: Option<&Path>,
    result: ItemResult,
    detail: Option<&str>,
) -> Result<(), CatalogError> {
    let destination = destination.map(path_to_text).transpose()?;
    connection
        .prepare_cached(
            "UPDATE file_operation_items
             SET destination_path = coalesce(?2, destination_path), result = ?3, detail = ?4
             WHERE id = ?1",
        )?
        .execute(params![item.0, destination, result_text(result), detail])?;
    Ok(())
}

/// Efface la ligne d'un fichier finalement laissé de côté sans rien faire
/// sur le disque (doublon reconnu à son empreinte : copie provisoire
/// abandonnée).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn forget(connection: &Connection, item: ItemId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM file_operation_items WHERE id = ?1")?
        .execute([item.0])?;
    Ok(())
}

/// Clôt les copies restées « en attente » sans être vérifiées, dans une
/// opération qui s'arrête (annulation, erreur) : rien n'a été nommé pour
/// elles, leur fichier provisoire a été effacé. Une copie vérifiée reste en
/// attente : elle a pu être nommée, la reprise le vérifiera.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn close_pending(
    connection: &Connection,
    operation: OperationId,
    detail: &str,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE file_operation_items SET result = 'failed', detail = ?2
             WHERE operation_id = ?1 AND result = 'pending' AND content_hash IS NULL",
        )?
        .execute(params![operation.0, detail])?;
    Ok(())
}

/// Opérations à terminer : restées « en cours » (l'application s'est
/// arrêtée pendant qu'elles se faisaient) ou avec des fichiers restés « en
/// attente » (catalogue illisible au mauvais moment, disque débranché).
/// À n'appeler que lorsqu'aucune opération ne se fait.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn interrupted(connection: &Connection) -> Result<Vec<InterruptedOperation>, CatalogError> {
    let mut statement = connection.prepare(
        "SELECT id, kind, import_id FROM file_operations
         WHERE status = 'running'
            OR EXISTS (SELECT 1 FROM file_operation_items
                       WHERE operation_id = file_operations.id AND result = 'pending')
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut operations = Vec::new();
    for row in rows {
        let (id, kind, import_id) = row?;
        let kind = match kind.as_str() {
            "trash" => OperationKind::Trash,
            "copy" => OperationKind::Copy,
            "move" => OperationKind::Move,
            "backup" => OperationKind::Backup,
            _ => {
                return Err(CatalogError::InvalidValue {
                    column: "file_operations.kind",
                    value: kind,
                });
            }
        };
        operations.push(InterruptedOperation {
            id: OperationId(id),
            kind,
            import_id: import_id.map(ImportId::new),
        });
    }
    Ok(operations)
}

/// Fichiers restés « en attente » d'une opération.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn pending_items(
    connection: &Connection,
    operation: OperationId,
) -> Result<Vec<PendingItem>, CatalogError> {
    let mut statement = connection.prepare(
        "SELECT id, source_path, destination_path, content_hash FROM file_operation_items
         WHERE operation_id = ?1 AND result = 'pending' ORDER BY id",
    )?;
    let items = statement
        .query_map([operation.0], |row| {
            Ok(PendingItem {
                id: ItemId(row.get(0)?),
                source: text_to_path(row.get(1)?),
                destination: row.get::<_, Option<String>>(2)?.map(text_to_path),
                content_hash: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(items)
}

/// Dossiers où une opération a écrit ou devait écrire (dossiers des copies,
/// et dossiers des copies en attente), chacun une fois.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn destination_folders(
    connection: &Connection,
    operation: OperationId,
) -> Result<Vec<PathBuf>, CatalogError> {
    let mut statement = connection.prepare(
        "SELECT destination_path, content_hash IS NULL AND result = 'pending'
         FROM file_operation_items
         WHERE operation_id = ?1 AND destination_path IS NOT NULL",
    )?;
    let rows = statement.query_map([operation.0], |row| {
        Ok((text_to_path(row.get(0)?), row.get::<_, bool>(1)?))
    })?;
    let mut folders: Vec<PathBuf> = Vec::new();
    for row in rows {
        let (path, is_folder) = row?;
        let folder = if is_folder {
            path
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        };
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    Ok(folders)
}

/// Clôt comme « en échec » une opération restée « en cours » après un arrêt
/// brutal ; sans effet sur une opération déjà terminée.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn close_interrupted(
    connection: &Connection,
    operation: OperationId,
    finished_at: Timestamp,
) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE file_operations SET status = 'failed', finished_at = ?2
         WHERE id = ?1 AND status = 'running'",
        params![operation.0, finished_at.unix_millis()],
    )?;
    Ok(())
}

/// Termine une opération.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn finish(
    connection: &Connection,
    operation: OperationId,
    status: OperationStatus,
    finished_at: Timestamp,
) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE file_operations SET status = ?2, finished_at = ?3 WHERE id = ?1",
        params![operation.0, status_text(status), finished_at.unix_millis()],
    )?;
    Ok(())
}

fn kind_text(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Trash => "trash",
        OperationKind::Copy => "copy",
        OperationKind::Move => "move",
        OperationKind::Backup => "backup",
    }
}

fn status_text(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Completed => "completed",
        OperationStatus::Cancelled => "cancelled",
        OperationStatus::Failed => "failed",
    }
}

fn result_text(result: ItemResult) -> &'static str {
    match result {
        ItemResult::Done => "done",
        ItemResult::Missing => "missing",
        ItemResult::Failed => "failed",
    }
}
