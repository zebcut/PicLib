//! Dossiers du catalogue.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::catalog::CatalogError;
use crate::catalog::sql::{media_kind_from_text, path_to_text, size_from_sql, text_to_path};
use crate::domain::{Folder, FolderId, MediaId, MediaKind, Timestamp};

const COLUMNS: &str = "id, parent_id, path";

/// Ajoute un dossier et renvoie son identifiant.
///
/// # Errors
///
/// Échoue si le dossier est déjà au catalogue, si le parent n'existe pas ou si
/// le chemin n'est pas en UTF-8.
pub fn insert(
    connection: &Connection,
    path: &Path,
    parent_id: Option<FolderId>,
) -> Result<FolderId, CatalogError> {
    connection.execute(
        "INSERT INTO folders (parent_id, path) VALUES (?1, ?2)",
        params![parent_id.map(FolderId::get), path_to_text(path)?],
    )?;
    Ok(FolderId::new(connection.last_insert_rowid()))
}

/// Rattache un dossier à son parent (dossier importé avant son parent).
///
/// # Errors
///
/// Échoue si le parent n'existe pas ou si la base est illisible.
pub fn set_parent(
    connection: &Connection,
    id: FolderId,
    parent_id: FolderId,
) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE folders SET parent_id = ?1 WHERE id = ?2",
        params![parent_id.get(), id.get()],
    )?;
    Ok(())
}

/// Cherche un dossier par son chemin exact.
///
/// # Errors
///
/// Échoue si la base est illisible ou si le chemin n'est pas en UTF-8.
pub fn find_by_path(connection: &Connection, path: &Path) -> Result<Option<Folder>, CatalogError> {
    let sql = format!("SELECT {COLUMNS} FROM folders WHERE path = ?1");
    Ok(connection
        .query_row(&sql, [path_to_text(path)?], folder_from_row)
        .optional()?)
}

/// Tous les dossiers du catalogue, triés par chemin.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<Folder>, CatalogError> {
    let sql = format!("SELECT {COLUMNS} FROM folders ORDER BY path");
    let mut statement = connection.prepare(&sql)?;
    let folders = statement
        .query_map([], folder_from_row)?
        .collect::<Result<_, _>>()?;
    Ok(folders)
}

/// Nombre de médias rangés directement dans chaque dossier (sous-dossiers
/// non compris) ; les dossiers sans média sont absents.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn media_counts(connection: &Connection) -> Result<Vec<(FolderId, u64)>, CatalogError> {
    let mut statement =
        connection.prepare_cached("SELECT folder_id, count(*) FROM media GROUP BY folder_id")?;
    let counts = statement
        .query_map([], |row| {
            Ok((FolderId::new(row.get(0)?), row.get::<_, i64>(1)?))
        })?
        .map(|row| {
            let (id, count) = row?;
            Ok((id, u64::try_from(count).unwrap_or(0)))
        })
        .collect::<Result<_, CatalogError>>()?;
    Ok(counts)
}

/// Retire du catalogue tous les dossiers qui ne contiennent aucun média,
/// sous-dossiers compris (le catalogue ne garde jamais un dossier sans photo ni vidéo, quelle qu'en soit la cause). Rien n'est
/// touché sur le disque. Renvoie le nombre de dossiers retirés. À appeler
/// dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn remove_without_media(connection: &Connection) -> Result<usize, CatalogError> {
    let all = list(connection)?;
    let parents: HashMap<FolderId, Option<FolderId>> = all
        .iter()
        .map(|folder| (folder.id, folder.parent_id))
        .collect();
    // Dossiers qui contiennent un média, et tous leurs parents.
    let mut kept: HashSet<FolderId> = HashSet::new();
    for (id, _) in media_counts(connection)? {
        let mut current = Some(id);
        // Au plus autant d'étapes que de dossiers : protège d'une boucle.
        for _ in 0..=parents.len() {
            let Some(folder) = current.filter(|folder| kept.insert(*folder)) else {
                break;
            };
            current = parents.get(&folder).copied().flatten();
        }
    }
    // Les plus profonds d'abord : un dossier ne se retire qu'une fois ses
    // sous-dossiers partis (clé étrangère `ON DELETE RESTRICT`).
    let mut removed: Vec<&Folder> = all
        .iter()
        .filter(|folder| !kept.contains(&folder.id))
        .collect();
    removed.sort_by_key(|folder| std::cmp::Reverse(folder.path.components().count()));
    let mut statement = connection.prepare_cached("DELETE FROM folders WHERE id = ?1")?;
    for folder in &removed {
        statement.execute([folder.id.get()])?;
    }
    if !removed.is_empty() {
        tracing::info!(
            count = removed.len(),
            "folders without media removed from the catalog"
        );
    }
    Ok(removed.len())
}

/// Dossiers des médias indiqués et tous leurs parents, tels qu'ils sont au
/// catalogue : de quoi les remettre si le retrait des médias les a vidés
/// (annulation).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn lineage_of(
    connection: &Connection,
    folder_ids: &[FolderId],
) -> Result<Vec<Folder>, CatalogError> {
    let all = list(connection)?;
    let by_id: HashMap<FolderId, &Folder> = all.iter().map(|folder| (folder.id, folder)).collect();
    let mut found: HashSet<FolderId> = HashSet::new();
    let mut lineage = Vec::new();
    for &id in folder_ids {
        let mut current = Some(id);
        for _ in 0..=by_id.len() {
            let Some(folder) = current.and_then(|id| by_id.get(&id)) else {
                break;
            };
            if !found.insert(folder.id) {
                break;
            }
            lineage.push((*folder).clone());
            current = folder.parent_id;
        }
    }
    Ok(lineage)
}

/// Remet au catalogue ceux des dossiers `folders` qui n'y sont plus, sous le
/// même identifiant, les parents d'abord. À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible, ou si un autre dossier a pris entre-temps
/// l'identifiant ou le chemin d'un dossier à remettre.
pub fn restore_missing(connection: &Connection, folders: &[Folder]) -> Result<(), CatalogError> {
    let mut ordered: Vec<&Folder> = folders.iter().collect();
    ordered.sort_by_key(|folder| folder.path.components().count());
    for folder in ordered {
        let exists: bool = connection
            .prepare_cached("SELECT EXISTS (SELECT 1 FROM folders WHERE id = ?1)")?
            .query_row([folder.id.get()], |row| row.get(0))?;
        if !exists {
            connection.execute(
                "INSERT INTO folders (id, parent_id, path) VALUES (?1, ?2, ?3)",
                params![
                    folder.id.get(),
                    folder.parent_id.map(FolderId::get),
                    path_to_text(&folder.path)?
                ],
            )?;
        }
    }
    Ok(())
}

/// Indique si `path`, ou un chemin sous lui, est déjà au catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible ou si le chemin n'est pas en UTF-8.
pub fn path_in_use(connection: &Connection, path: &Path) -> Result<bool, CatalogError> {
    let text = path_to_text(path)?;
    // `substr` plutôt que `LIKE` : aucun caractère du chemin n'est spécial.
    Ok(connection
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM folders
             WHERE path = ?1 OR substr(path, 1, length(?1) + 1) = ?1 || '/')",
        )?
        .query_row([text], |row| row.get(0))?)
}

/// Donne au dossier `id` le chemin `new_path` et le parent `parent_id`, et
/// fait suivre les chemins de ses sous-dossiers (dossier déplacé). Les
/// médias suivent d'eux-mêmes : ils sont rangés par dossier et par nom. À
/// appeler dans une transaction.
///
/// # Errors
///
/// Échoue si le dossier n'est pas au catalogue, si un chemin est déjà pris
/// ou n'est pas en UTF-8, ou si la base ne peut pas être écrite.
pub fn relocate(
    connection: &Connection,
    id: FolderId,
    new_path: &Path,
    parent_id: Option<FolderId>,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(&format!(
        "{BRANCH} SELECT folders.id, folders.path FROM branch JOIN folders USING (id)"
    ))?;
    let branch = statement
        .query_map([id.get()], |row| {
            Ok((FolderId::new(row.get(0)?), text_to_path(row.get(1)?)))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let old_root = branch
        .iter()
        .find(|(folder, _)| *folder == id)
        .map(|(_, path)| path.clone())
        .ok_or(CatalogError::NotFound {
            entity: "folder",
            id: id.get(),
        })?;
    let mut update = connection.prepare_cached("UPDATE folders SET path = ?2 WHERE id = ?1")?;
    for (folder, path) in &branch {
        let relative = path.strip_prefix(&old_root).unwrap_or(Path::new(""));
        let moved = if relative.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(relative)
        };
        update.execute(params![folder.get(), path_to_text(&moved)?])?;
    }
    connection.execute(
        "UPDATE folders SET parent_id = ?2 WHERE id = ?1",
        params![id.get(), parent_id.map(FolderId::get)],
    )?;
    Ok(())
}

/// Dossier du catalogue pour `path`, ajouté s'il n'y est pas, rattaché au
/// plus proche de ses parents qui y est (les dossiers intermédiaires sont
/// ajoutés aussi ; sans parent connu, il est ajouté seul). À appeler dans
/// une transaction.
///
/// # Errors
///
/// Échoue si un chemin n'est pas en UTF-8 ou si la base ne peut pas être
/// écrite.
pub fn ensure_path(connection: &Connection, path: &Path) -> Result<FolderId, CatalogError> {
    if let Some(folder) = find_by_path(connection, path)? {
        return Ok(folder.id);
    }
    let mut missing = vec![path];
    let mut parent = None;
    for ancestor in path.ancestors().skip(1) {
        if let Some(folder) = find_by_path(connection, ancestor)? {
            parent = Some(folder.id);
            break;
        }
        missing.push(ancestor);
    }
    if parent.is_none() {
        // Aucun parent connu : seul le dossier lui-même est ajouté.
        missing.truncate(1);
    }
    let mut id = parent;
    for folder in missing.into_iter().rev() {
        id = Some(insert(connection, folder, id)?);
    }
    id.ok_or(CatalogError::NotFound {
        entity: "folder",
        id: 0,
    })
}

/// Média d'une branche de dossiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchMedia {
    /// Identifiant.
    pub id: MediaId,
    /// Chemin complet du fichier.
    pub path: std::path::PathBuf,
    /// Photo ou vidéo.
    pub kind: MediaKind,
    /// Taille du fichier, en octets.
    pub file_size: u64,
    /// Date de modification du fichier, à l'import.
    pub file_modified_at: Timestamp,
}

/// Dossiers de la branche de `id` (lui compris), parcourus par l'index
/// `folders_by_parent` ; `UNION` arrête le parcours même en cas de boucle.
const BRANCH: &str = "WITH RECURSIVE branch(id) AS (
        SELECT ?1
        UNION
        SELECT folders.id FROM folders JOIN branch ON folders.parent_id = branch.id
    )";

/// Nombre de dossiers de la branche de `id`, lui compris.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn branch_size(connection: &Connection, id: FolderId) -> Result<u64, CatalogError> {
    let count: i64 = connection
        .prepare_cached(&format!(
            "{BRANCH} SELECT count(*) FROM branch JOIN folders USING (id)"
        ))?
        .query_row([id.get()], |row| row.get(0))?;
    size_from_sql(count)
}

/// Médias du dossier `id` et de ses sous-dossiers, avec leur chemin.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn media_in_branch(
    connection: &Connection,
    id: FolderId,
) -> Result<Vec<BranchMedia>, CatalogError> {
    let mut statement = connection.prepare_cached(&format!(
        "{BRANCH}
         SELECT media.id, folders.path, media.file_name, media.kind, media.file_size,
             media.file_modified_at
         FROM media JOIN folders ON folders.id = media.folder_id
         WHERE media.folder_id IN (SELECT id FROM branch)
         ORDER BY media.id"
    ))?;
    let rows = statement.query_map([id.get()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut media = Vec::new();
    for row in rows {
        let (id, folder, file_name, kind, file_size, modified) = row?;
        media.push(BranchMedia {
            id: MediaId::new(id),
            path: text_to_path(folder).join(file_name),
            kind: media_kind_from_text(&kind)?,
            file_size: size_from_sql(file_size)?,
            file_modified_at: Timestamp::from_unix_millis(modified),
        });
    }
    Ok(media)
}

fn folder_from_row(row: &Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: FolderId::new(row.get("id")?),
        parent_id: row.get::<_, Option<i64>>("parent_id")?.map(FolderId::new),
        path: text_to_path(row.get("path")?),
    })
}
