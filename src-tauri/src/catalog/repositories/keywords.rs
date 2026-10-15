//! Mots-clés hiérarchiques et mots-clés des médias (migration 11).

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders};
use crate::domain::{KeywordId, MediaId, keyword_name_key, keyword_search_key};

/// Un mot-clé de l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword {
    /// Identifiant.
    pub id: KeywordId,
    /// Parent ; `None` à la racine.
    pub parent_id: Option<KeywordId>,
    /// Nom tel que saisi.
    pub name: String,
    /// « Inclure à l'export ».
    pub include_on_export: bool,
}

/// Mot-clé désigné par un nom, au premier niveau d'un chemin saisi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstLevel {
    /// Un seul mot-clé correspond.
    Found(KeywordId),
    /// Aucun mot-clé ne porte ce nom.
    Missing,
    /// Plusieurs mots-clés portent ce nom, aucun à la racine.
    Ambiguous,
}

/// Où chercher les suggestions de l'autocomplétion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionScope {
    /// Dans tout l'arbre.
    Anywhere,
    /// Parmi les enfants directs de ce mot-clé.
    ChildrenOf(KeywordId),
}

/// Tous les mots-clés : l'arbre entier tient en mémoire, même avec des
/// dizaines de milliers de mots-clés (22 759 lus en 5 ms).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn all(connection: &Connection) -> Result<Vec<Keyword>, CatalogError> {
    let mut statement =
        connection.prepare_cached("SELECT id, parent_id, name, include_on_export FROM keywords")?;
    let rows = statement.query_map([], |row| {
        Ok(Keyword {
            id: KeywordId::new(row.get(0)?),
            parent_id: row.get::<_, Option<i64>>(1)?.map(KeywordId::new),
            name: row.get(2)?,
            include_on_export: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Mot-clé désigné par un nom seul : celui de la racine s'il
/// existe, sinon le seul qui porte ce nom ailleurs dans l'arbre.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn first_level(connection: &Connection, name: &str) -> Result<FirstLevel, CatalogError> {
    // Celui de la racine d'abord ; deux lignes suffisent pour savoir si le
    // nom est ambigu.
    let mut statement = connection.prepare_cached(
        "SELECT id, parent_id IS NULL FROM keywords WHERE name_key = ?1
         ORDER BY parent_id IS NOT NULL LIMIT 2",
    )?;
    let found: Vec<(i64, bool)> = statement
        .query_map([keyword_name_key(name)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(match found.as_slice() {
        [] => FirstLevel::Missing,
        [(id, _)] | [(id, true), ..] => FirstLevel::Found(KeywordId::new(*id)),
        _ => FirstLevel::Ambiguous,
    })
}

/// Enfant de `parent` (`None` : la racine) qui porte ce nom.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn child(
    connection: &Connection,
    parent: Option<KeywordId>,
    name: &str,
) -> Result<Option<KeywordId>, CatalogError> {
    let id = connection
        .prepare_cached(
            "SELECT id FROM keywords WHERE coalesce(parent_id, 0) = ?1 AND name_key = ?2",
        )?
        .query_row(
            params![parent.map_or(0, KeywordId::get), keyword_name_key(name)],
            |row| row.get(0),
        )
        .optional()?;
    Ok(id.map(KeywordId::new))
}

/// Identifiant que recevra le prochain mot-clé créé : il dépasse le plus
/// grand jamais attribué (table `id_watermarks`, migration n° 12).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn next_id(connection: &Connection) -> Result<KeywordId, CatalogError> {
    let id = connection
        .prepare_cached(
            "SELECT max(coalesce((SELECT max(id) FROM keywords), 0), last_id) + 1
             FROM id_watermarks WHERE name = 'keywords'",
        )?
        .query_row([], |row| row.get(0))?;
    Ok(KeywordId::new(id))
}

/// Crée un mot-clé sous `parent` (`None` : à la racine).
///
/// # Errors
///
/// Échoue si ce parent a déjà un enfant de ce nom, ou si la base ne peut
/// pas être écrite.
pub fn create(
    connection: &Connection,
    parent: Option<KeywordId>,
    name: &str,
) -> Result<KeywordId, CatalogError> {
    // L'identifiant n'est jamais celui d'un mot-clé supprimé : il dépasse le
    // plus grand jamais attribué (table `id_watermarks`, migration n° 12).
    connection
        .prepare_cached(
            "INSERT INTO keywords (id, parent_id, name, name_key, search_key)
             VALUES ((SELECT max(coalesce((SELECT max(id) FROM keywords), 0), last_id) + 1
                      FROM id_watermarks WHERE name = 'keywords'),
                     ?1, ?2, ?3, ?4)",
        )?
        .execute(params![
            parent.map(KeywordId::get),
            name,
            keyword_name_key(name),
            keyword_search_key(name)
        ])?;
    Ok(KeywordId::new(connection.last_insert_rowid()))
}

/// Mots-clés dont le nom contient `search` (clé de recherche, sans accents
/// ni majuscules), avec leur nombre de médias : ceux qui commencent par le
/// texte d'abord, puis les plus utilisés, puis l'ordre alphabétique.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn suggest(
    connection: &Connection,
    scope: SuggestionScope,
    search: &str,
    limit: usize,
) -> Result<Vec<(KeywordId, u64)>, CatalogError> {
    let key = keyword_search_key(search);
    let parent = match scope {
        SuggestionScope::Anywhere => None,
        SuggestionScope::ChildrenOf(parent) => Some(parent.get()),
    };
    let mut statement = connection.prepare_cached(
        "SELECT keywords.id,
                (SELECT count(*) FROM media_keywords WHERE keyword_id = keywords.id) AS uses
         FROM keywords
         WHERE instr(search_key, ?1) > 0 AND (?2 IS NULL OR parent_id = ?2)
         ORDER BY instr(search_key, ?1) = 1 DESC, uses DESC, search_key, id
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![key, parent, i64::try_from(limit).unwrap_or(i64::MAX)],
        |row| Ok((KeywordId::new(row.get(0)?), row.get::<_, i64>(1)?)),
    )?;
    rows.map(|row| {
        let (id, uses) = row?;
        Ok((id, u64::try_from(uses).unwrap_or(0)))
    })
    .collect()
}

/// Médias existants parmi `ids` (les identifiants inconnus sont écartés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn existing_media(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<MediaId>, CatalogError> {
    let mut existing = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok(MediaId::new(row.get(0)?))
            })?;
        for row in rows {
            existing.push(row?);
        }
    }
    Ok(existing)
}

/// Mots-clés existants parmi `ids`, dans leur ordre (les supprimés sont
/// écartés).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn existing_keywords(
    connection: &Connection,
    ids: &[KeywordId],
) -> Result<Vec<KeywordId>, CatalogError> {
    let mut statement = connection.prepare_cached("SELECT 1 FROM keywords WHERE id = ?1")?;
    let mut existing = Vec::with_capacity(ids.len());
    for &id in ids {
        if statement.exists([id.get()])? {
            existing.push(id);
        }
    }
    Ok(existing)
}

/// Mots-clés saisis sur les médias `ids` : une paire par lien.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn links(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, KeywordId)>, CatalogError> {
    let mut links = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media_id, keyword_id FROM media_keywords WHERE media_id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((MediaId::new(row.get(0)?), KeywordId::new(row.get(1)?)))
            })?;
        for row in rows {
            links.push(row?);
        }
    }
    Ok(links)
}

/// Tous les liens (média, mot-clé saisi) du catalogue, triés par média
/// puis par mot-clé (l'ordre de la table : lecture sans tri).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn all_links(connection: &Connection) -> Result<Vec<(MediaId, KeywordId)>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT media_id, keyword_id FROM media_keywords ORDER BY media_id, keyword_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((MediaId::new(row.get(0)?), KeywordId::new(row.get(1)?)))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Liens (média, mot-clé) des mots-clés `ids`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn links_of_keywords(
    connection: &Connection,
    ids: &[KeywordId],
) -> Result<Vec<(MediaId, KeywordId)>, CatalogError> {
    let mut links = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media_id, keyword_id FROM media_keywords WHERE keyword_id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((MediaId::new(row.get(0)?), KeywordId::new(row.get(1)?)))
            })?;
        for row in rows {
            links.push(row?);
        }
    }
    Ok(links)
}

/// Pose les mots-clés indiqués (un lien déjà présent est ignoré). À appeler
/// dans une transaction.
///
/// # Errors
///
/// Échoue si un média ou un mot-clé n'existe pas, ou si la base ne peut pas
/// être écrite.
pub fn link(connection: &Connection, links: &[(MediaId, KeywordId)]) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "INSERT OR IGNORE INTO media_keywords (media_id, keyword_id) VALUES (?1, ?2)",
    )?;
    for (media, keyword) in links {
        statement.execute(params![media.get(), keyword.get()])?;
    }
    Ok(())
}

/// Retire les mots-clés indiqués. À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn unlink(connection: &Connection, links: &[(MediaId, KeywordId)]) -> Result<(), CatalogError> {
    let mut statement = connection
        .prepare_cached("DELETE FROM media_keywords WHERE media_id = ?1 AND keyword_id = ?2")?;
    for (media, keyword) in links {
        statement.execute(params![media.get(), keyword.get()])?;
    }
    Ok(())
}
