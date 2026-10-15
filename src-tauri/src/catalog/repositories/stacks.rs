//! Piles : des photos d'un même dossier regroupées
//! sous une couverture. Une photo n'appartient qu'à une pile ; la
//! couverture est la photo de plus petit rang. Une pile repliée n'affiche
//! qu'une vignette dans la grille (voir [`hidden_condition`]).

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, media_format_from_text, placeholders};
use crate::domain::{FolderId, ImportId, MediaFormat, MediaId, StackId};

/// Une pile et ses photos, la couverture en premier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    /// Identifiant.
    pub id: StackId,
    /// Repliée : une seule vignette dans la grille.
    pub collapsed: bool,
    /// Photos, dans l'ordre de la pile.
    pub members: Vec<MediaId>,
}

impl Stack {
    /// Photo de couverture.
    #[must_use]
    pub fn cover(&self) -> Option<MediaId> {
        self.members.first().copied()
    }
}

/// Photo qui pourrait rejoindre une pile, avec ce qu'il faut pour la
/// ranger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackCandidate {
    /// Photo.
    pub id: MediaId,
    /// Dossier.
    pub folder_id: FolderId,
    /// Date de prise de vue locale (`2024-07-14T18:30:05`).
    pub capture_time: String,
    /// Nom du fichier.
    pub file_name: String,
    /// Pile où elle est déjà.
    pub stack: Option<StackId>,
}

/// Les photos `ids` qui existent, avec leur dossier et leur pile, dans
/// l'ordre de `ids`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn candidates(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<StackCandidate>, CatalogError> {
    let mut found = std::collections::HashMap::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media.id, media.folder_id, media.capture_time, media.file_name,
                stack_media.stack_id
             FROM media LEFT JOIN stack_media ON stack_media.media_id = media.id
             WHERE media.id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows =
            statement.query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok(StackCandidate {
                    id: MediaId::new(row.get(0)?),
                    folder_id: FolderId::new(row.get(1)?),
                    capture_time: row.get(2)?,
                    file_name: row.get(3)?,
                    stack: row.get::<_, Option<i64>>(4)?.map(StackId::new),
                })
            })?;
        for row in rows {
            let candidate = row?;
            found.insert(candidate.id, candidate);
        }
    }
    let mut seen = std::collections::HashSet::new();
    Ok(ids
        .iter()
        .filter(|id| seen.insert(**id))
        .filter_map(|id| found.remove(id))
        .collect())
}

/// Photo examinée par l'empilement automatique (paires RAW + JPEG,
/// rafales).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoCandidate {
    /// Photo.
    pub id: MediaId,
    /// Dossier.
    pub folder_id: FolderId,
    /// Nom du fichier.
    pub file_name: String,
    /// Format.
    pub format: MediaFormat,
    /// Date de prise de vue locale (`2024-07-14T18:30:05`).
    pub capture_time: String,
    /// Pile où elle est déjà, et son nombre de photos.
    pub stack: Option<(StackId, u32)>,
}

/// Photos examinées par l'empilement automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateScope<'a> {
    /// Les photos ajoutées par un import.
    Import(ImportId),
    /// Les photos d'un dossier et de ses sous-dossiers.
    FolderTree(FolderId),
    /// Les photos de ces dossiers (sans leurs sous-dossiers).
    Folders(&'a [FolderId]),
    /// Ces photos.
    Media(&'a [MediaId]),
}

/// Photos (pas les vidéos) de `scope`, par dossier puis par date de prise
/// de vue.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn auto_candidates(
    connection: &Connection,
    scope: CandidateScope<'_>,
) -> Result<Vec<AutoCandidate>, CatalogError> {
    let mut found = match scope {
        CandidateScope::Import(import) => {
            read_candidates(connection, "media.import_id = ?1", &[import.get()])?
        }
        CandidateScope::FolderTree(folder) => read_candidates(
            connection,
            "media.folder_id IN (
                WITH RECURSIVE branch(id) AS (
                    SELECT ?1
                    UNION
                    SELECT folders.id FROM folders JOIN branch ON folders.parent_id = branch.id
                )
                SELECT id FROM branch)",
            &[folder.get()],
        )?,
        CandidateScope::Folders(folders) => {
            let values: Vec<i64> = folders.iter().map(|id| id.get()).collect();
            read_chunked(connection, "media.folder_id", &values)?
        }
        CandidateScope::Media(ids) => {
            let values: Vec<i64> = ids.iter().map(|id| id.get()).collect();
            read_chunked(connection, "media.id", &values)?
        }
    };
    found.sort_by(|a, b| {
        (a.folder_id, &a.capture_time, &a.file_name).cmp(&(
            b.folder_id,
            &b.capture_time,
            &b.file_name,
        ))
    });
    Ok(found)
}

fn read_chunked(
    connection: &Connection,
    column: &str,
    values: &[i64],
) -> Result<Vec<AutoCandidate>, CatalogError> {
    let mut found = Vec::new();
    for chunk in values.chunks(IDS_PER_QUERY) {
        let condition = format!("{column} IN ({})", placeholders(1, chunk.len()));
        found.extend(read_candidates(connection, &condition, chunk)?);
    }
    Ok(found)
}

fn read_candidates(
    connection: &Connection,
    condition: &str,
    values: &[i64],
) -> Result<Vec<AutoCandidate>, CatalogError> {
    let sql = format!(
        "SELECT media.id, media.folder_id, media.file_name, media.format, media.capture_time,
            stack_media.stack_id,
            (SELECT count(*) FROM stack_media AS other
             WHERE other.stack_id = stack_media.stack_id)
         FROM media LEFT JOIN stack_media ON stack_media.media_id = media.id
         WHERE media.kind = 'photo' AND {condition}"
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(id, folder, file_name, format, capture_time, stack, size)| {
                Ok(AutoCandidate {
                    id: MediaId::new(id),
                    folder_id: FolderId::new(folder),
                    file_name,
                    format: media_format_from_text(&format)?,
                    capture_time,
                    stack: stack.map(|stack| {
                        (StackId::new(stack), u32::try_from(size).unwrap_or(u32::MAX))
                    }),
                })
            },
        )
        .collect()
}

/// Une pile ; `None` si elle n'existe pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn get(connection: &Connection, id: StackId) -> Result<Option<Stack>, CatalogError> {
    let collapsed: Option<bool> = connection
        .prepare_cached("SELECT collapsed FROM stacks WHERE id = ?1")?
        .query_row([id.get()], |row| row.get(0))
        .optional()?;
    let Some(collapsed) = collapsed else {
        return Ok(None);
    };
    let members = connection
        .prepare_cached(
            "SELECT media_id FROM stack_media WHERE stack_id = ?1 ORDER BY position, media_id",
        )?
        .query_map([id.get()], |row| row.get(0).map(MediaId::new))?
        .collect::<Result<_, _>>()?;
    Ok(Some(Stack {
        id,
        collapsed,
        members,
    }))
}

/// Les piles où sont les photos `ids`, chacune une fois, dans l'ordre où
/// elles apparaissent dans `ids`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn of_media(connection: &Connection, ids: &[MediaId]) -> Result<Vec<Stack>, CatalogError> {
    let mut stack_ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for candidate in candidates(connection, ids)? {
        if let Some(stack) = candidate.stack
            && seen.insert(stack)
        {
            stack_ids.push(stack);
        }
    }
    let mut stacks = Vec::with_capacity(stack_ids.len());
    for id in stack_ids {
        if let Some(stack) = get(connection, id)? {
            stacks.push(stack);
        }
    }
    Ok(stacks)
}

/// Identifiant d'une nouvelle pile, choisi avant de la créer : une
/// annulation puis un rétablissement la recréent sous le même.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn next_id(connection: &Connection) -> Result<StackId, CatalogError> {
    let id: i64 = connection
        .prepare_cached("SELECT coalesce(max(id), 0) + 1 FROM stacks")?
        .query_row([], |row| row.get(0))?;
    Ok(StackId::new(id))
}

/// Crée une pile, avec son identifiant et ses photos (celles qui ne sont
/// plus au catalogue sont ignorées). À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible, si la pile existe déjà ou si une photo
/// est dans une autre pile.
pub fn insert(connection: &Connection, stack: &Stack) -> Result<(), CatalogError> {
    connection
        .prepare_cached("INSERT INTO stacks (id, collapsed) VALUES (?1, ?2)")?
        .execute(params![stack.id.get(), stack.collapsed])?;
    let mut statement = connection.prepare_cached(
        "INSERT INTO stack_media (media_id, stack_id, position)
         SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)",
    )?;
    for (position, member) in (0_i64..).zip(&stack.members) {
        statement.execute(params![member.get(), stack.id.get(), position])?;
    }
    Ok(())
}

/// Supprime une pile (ses photos restent au catalogue).
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn delete(connection: &Connection, id: StackId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM stacks WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}

/// Replie ou déplie des piles : un état d'affichage, hors de Cmd+Z.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn set_collapsed(
    connection: &Connection,
    ids: &[StackId],
    collapsed: bool,
) -> Result<(), CatalogError> {
    let mut statement =
        connection.prepare_cached("UPDATE stacks SET collapsed = ?2 WHERE id = ?1")?;
    for id in ids {
        statement.execute(params![id.get(), collapsed])?;
    }
    Ok(())
}

/// Vrai s'il y a au moins une pile repliée : la grille doit alors cacher
/// les photos qu'elles recouvrent.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn has_collapsed(connection: &Connection) -> Result<bool, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM stacks WHERE collapsed = 1)")?
        .query_row([], |row| row.get(0))?)
}

/// Supprime les piles de moins de deux photos (gardées pendant une session
/// pour ses annulations).
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn remove_lone(connection: &Connection) -> Result<usize, CatalogError> {
    Ok(connection.execute(
        "DELETE FROM stacks
         WHERE (SELECT count(*) FROM stack_media WHERE stack_media.stack_id = stacks.id) < 2",
        [],
    )?)
}

/// Condition qui écarte de la grille les photos recouvertes par une pile
/// repliée. `matching` est la condition de la grille (source et filtres) :
/// parmi les photos d'une pile repliée qui y répondent, seule la première
/// dans l'ordre de la pile reste, la couverture si elle y répond.
pub(super) fn hidden_condition(matching: &str) -> String {
    format!(
        "media.id NOT IN (
            SELECT ranked.media_id FROM (
                SELECT stack_media.media_id, row_number() OVER (
                    PARTITION BY stack_media.stack_id
                    ORDER BY stack_media.position, stack_media.media_id
                ) AS rank
                FROM stack_media JOIN stacks ON stacks.id = stack_media.stack_id
                WHERE stacks.collapsed = 1
                    AND stack_media.media_id IN (SELECT media.id FROM media WHERE {matching})
            ) AS ranked
            WHERE ranked.rank > 1)"
    )
}

/// Condition des photos d'une pile d'au moins deux photos (filtre
/// Attribut « Pile », règle « appartenance à une pile »).
pub(super) const IN_STACK: &str = "media.id IN (
    SELECT stack_media.media_id FROM stack_media
    WHERE stack_media.stack_id IN (
        SELECT stack_id FROM stack_media GROUP BY stack_id HAVING count(*) >= 2))";

/// Jointures qui donnent sa pile à chaque ligne de `media` (alias
/// `stacked` et `stack`), pour [`PLACE_COLUMNS`].
pub(super) const PLACE_JOINS: &str =
    "LEFT JOIN stack_media AS stacked ON stacked.media_id = media.id
    LEFT JOIN stacks AS stack ON stack.id = stacked.stack_id";

/// Pile, repliée ou non, nombre de photos et rang (à partir de 1) de la
/// photo dans sa pile ; `NULL`, 0 et 1 hors d'une pile.
pub(super) const PLACE_COLUMNS: &str = "stacked.stack_id, stack.collapsed,
    (SELECT count(*) FROM stack_media AS other WHERE other.stack_id = stacked.stack_id),
    (SELECT count(*) FROM stack_media AS other WHERE other.stack_id = stacked.stack_id
        AND (other.position < stacked.position
            OR (other.position = stacked.position AND other.media_id < stacked.media_id))) + 1";
