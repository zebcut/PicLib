//! Modifications de l'arbre des mots-clés : insertion avec un
//! identifiant connu (pour annuler une suppression), suppression, nom,
//! parent et option « inclure à l'export ». À appeler dans une transaction.

use rusqlite::{Connection, params};

use super::keywords::Keyword;
use crate::catalog::CatalogError;
use crate::domain::{KeywordId, keyword_name_key, keyword_search_key};

/// Remet un mot-clé tel qu'il était, avec son identifiant : les liens des
/// photos qui le désignaient redeviennent valables.
///
/// # Errors
///
/// Échoue si l'identifiant ou le nom (sous ce parent) est déjà pris, ou si
/// la base ne peut pas être écrite.
pub fn insert(connection: &Connection, keyword: &Keyword) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO keywords (id, parent_id, name, name_key, search_key, include_on_export)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?
        .execute(params![
            keyword.id.get(),
            keyword.parent_id.map(KeywordId::get),
            keyword.name,
            keyword_name_key(&keyword.name),
            keyword_search_key(&keyword.name),
            keyword.include_on_export
        ])?;
    Ok(())
}

/// Supprime un mot-clé. Ses enfants doivent avoir été supprimés ou déplacés
/// avant ; ses liens avec des photos disparaissent avec lui.
///
/// # Errors
///
/// Échoue s'il a encore des enfants, ou si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: KeywordId) -> Result<(), CatalogError> {
    // Retient d'abord le plus grand identifiant attribué, pour qu'il ne soit
    // jamais redonné à un nouveau mot-clé (migration n° 12).
    connection
        .prepare_cached(
            "UPDATE id_watermarks
             SET last_id = max(last_id, coalesce((SELECT max(id) FROM keywords), 0))
             WHERE name = 'keywords'",
        )?
        .execute([])?;
    connection
        .prepare_cached("DELETE FROM keywords WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}

/// Donne un nouveau nom à un mot-clé.
///
/// # Errors
///
/// Échoue si son parent a déjà un autre enfant de ce nom, ou si la base ne
/// peut pas être écrite.
pub fn rename(connection: &Connection, id: KeywordId, name: &str) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE keywords SET name = ?2, name_key = ?3, search_key = ?4 WHERE id = ?1",
        )?
        .execute(params![
            id.get(),
            name,
            keyword_name_key(name),
            keyword_search_key(name)
        ])?;
    Ok(())
}

/// Range un mot-clé sous un autre parent (`None` : à la racine), avec toute
/// sa branche.
///
/// # Errors
///
/// Échoue si ce parent a déjà un enfant de ce nom, ou si la base ne peut
/// pas être écrite.
pub fn set_parent(
    connection: &Connection,
    id: KeywordId,
    parent: Option<KeywordId>,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE keywords SET parent_id = ?2 WHERE id = ?1")?
        .execute(params![id.get(), parent.map(KeywordId::get)])?;
    Ok(())
}

/// Coche ou décoche « inclure à l'export ».
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_include_on_export(
    connection: &Connection,
    id: KeywordId,
    include: bool,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE keywords SET include_on_export = ?2 WHERE id = ?1")?
        .execute(params![id.get(), include])?;
    Ok(())
}
