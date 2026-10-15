//! Jeux de mots-clés (migration 13) : nom, jeu actif et neuf cases. À
//! appeler dans une transaction pour les écritures en plusieurs requêtes.

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders};
use crate::domain::{KeywordId, KeywordSetId, keyword_name_key};

/// Nombre de cases d'un jeu (Option+1 à Option+9).
pub const SLOT_COUNT: usize = 9;

/// Case d'un jeu : le jeu et le numéro de la case, de 1 à 9.
pub type SlotRef = (KeywordSetId, u8);

/// Un jeu de mots-clés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordSet {
    /// Identifiant.
    pub id: KeywordSetId,
    /// Nom.
    pub name: String,
    /// Vrai pour le jeu affiché dans le panneau et servi par les raccourcis.
    pub active: bool,
    /// Mot-clé de chaque case (la case 1 d'abord) ; `None` : case vide, ou
    /// dont le mot-clé a été supprimé.
    pub slots: [Option<KeywordId>; SLOT_COUNT],
}

/// Tous les jeux, dans l'ordre de leur nom.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn all(connection: &Connection) -> Result<Vec<KeywordSet>, CatalogError> {
    let mut sets: Vec<KeywordSet> = connection
        .prepare_cached("SELECT id, name, active FROM keyword_sets ORDER BY name_key")?
        .query_map([], |row| {
            Ok(KeywordSet {
                id: KeywordSetId::new(row.get(0)?),
                name: row.get(1)?,
                active: row.get(2)?,
                slots: [None; SLOT_COUNT],
            })
        })?
        .collect::<Result<_, _>>()?;
    // Seules les cases dont le mot-clé existe encore.
    let mut statement = connection.prepare_cached(
        "SELECT set_id, slot, keyword_id FROM keyword_set_slots
         JOIN keywords ON keywords.id = keyword_set_slots.keyword_id",
    )?;
    let slots = statement.query_map([], |row| {
        Ok((
            KeywordSetId::new(row.get(0)?),
            row.get::<_, i64>(1)?,
            KeywordId::new(row.get(2)?),
        ))
    })?;
    for slot in slots {
        let (set_id, slot, keyword) = slot?;
        let index = usize::try_from(slot - 1).ok();
        if let Some(cell) = sets
            .iter_mut()
            .find(|set| set.id == set_id)
            .zip(index)
            .and_then(|(set, index)| set.slots.get_mut(index))
        {
            *cell = Some(keyword);
        }
    }
    Ok(sets)
}

/// Jeu qui porte ce nom (sans tenir compte des majuscules).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_by_name(
    connection: &Connection,
    name: &str,
) -> Result<Option<KeywordSetId>, CatalogError> {
    let id = connection
        .prepare_cached("SELECT id FROM keyword_sets WHERE name_key = ?1")?
        .query_row([keyword_name_key(name)], |row| row.get(0))
        .optional()?;
    Ok(id.map(KeywordSetId::new))
}

/// Vrai si le jeu existe.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn exists(connection: &Connection, id: KeywordSetId) -> Result<bool, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT 1 FROM keyword_sets WHERE id = ?1")?
        .query_row([id.get()], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Crée un jeu vide, inactif.
///
/// # Errors
///
/// Échoue si le nom est déjà pris, ou si la base ne peut pas être écrite.
pub fn create(connection: &Connection, name: &str) -> Result<KeywordSetId, CatalogError> {
    connection
        .prepare_cached("INSERT INTO keyword_sets (name, name_key) VALUES (?1, ?2)")?
        .execute(params![name, keyword_name_key(name)])?;
    Ok(KeywordSetId::new(connection.last_insert_rowid()))
}

/// Donne un nouveau nom à un jeu.
///
/// # Errors
///
/// Échoue si le nom est déjà pris, ou si la base ne peut pas être écrite.
pub fn rename(connection: &Connection, id: KeywordSetId, name: &str) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE keyword_sets SET name = ?2, name_key = ?3 WHERE id = ?1")?
        .execute(params![id.get(), name, keyword_name_key(name)])?;
    Ok(())
}

/// Supprime un jeu et ses cases (les mots-clés restent).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: KeywordSetId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM keyword_sets WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}

/// Rend un jeu actif (`None` : aucun), à la place du précédent.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_active(connection: &Connection, id: Option<KeywordSetId>) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE keyword_sets SET active = 0 WHERE active = 1")?
        .execute([])?;
    if let Some(id) = id {
        connection
            .prepare_cached("UPDATE keyword_sets SET active = 1 WHERE id = ?1")?
            .execute([id.get()])?;
    }
    Ok(())
}

/// Remplace les neuf cases d'un jeu.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn replace_slots(
    connection: &Connection,
    id: KeywordSetId,
    slots: &[Option<KeywordId>; SLOT_COUNT],
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM keyword_set_slots WHERE set_id = ?1")?
        .execute([id.get()])?;
    let mut insert = connection.prepare_cached(
        "INSERT INTO keyword_set_slots (set_id, slot, keyword_id) VALUES (?1, ?2, ?3)",
    )?;
    for (slot, keyword) in (1_i64..).zip(slots) {
        if let Some(keyword) = keyword {
            insert.execute(params![id.get(), slot, keyword.get()])?;
        }
    }
    Ok(())
}

/// Cases qui désignent l'un des mots-clés `ids`, avec ce mot-clé.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn slots_of_keywords(
    connection: &Connection,
    ids: &[KeywordId],
) -> Result<Vec<(SlotRef, KeywordId)>, CatalogError> {
    let mut slots = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT set_id, slot, keyword_id FROM keyword_set_slots WHERE keyword_id IN ({})",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows =
            statement.query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                Ok((
                    (KeywordSetId::new(row.get(0)?), row.get(1)?),
                    KeywordId::new(row.get(2)?),
                ))
            })?;
        for row in rows {
            slots.push(row?);
        }
    }
    Ok(slots)
}

/// Fait désigner `keyword` aux cases `slots` (fusion d'un mot-clé).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn retarget(
    connection: &Connection,
    slots: &[SlotRef],
    keyword: KeywordId,
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "UPDATE keyword_set_slots SET keyword_id = ?3 WHERE set_id = ?1 AND slot = ?2",
    )?;
    for (set, slot) in slots {
        statement.execute(params![set.get(), slot, keyword.get()])?;
    }
    Ok(())
}
