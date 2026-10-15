//! Collections manuelles et dossiers de collections :
//! l'arbre, et les photos de chaque collection.

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{IDS_PER_QUERY, placeholders, size_from_sql};
use crate::domain::{CollectionId, MediaId, keyword_name_key};

/// Collection, ou dossier qui en range d'autres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    /// Collection de photos.
    Collection,
    /// Dossier de collections.
    Folder,
}

impl CollectionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Collection => "collection",
            Self::Folder => "folder",
        }
    }
}

/// Collection ou dossier de collections, à sa place dans l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionNode {
    /// Identifiant.
    pub id: CollectionId,
    /// Dossier parent ; `None` à la racine.
    pub parent_id: Option<CollectionId>,
    /// Collection ou dossier.
    pub kind: CollectionKind,
    /// Nom.
    pub name: String,
    /// Règles d'une collection dynamique (JSON) ; `None` pour une collection
    /// manuelle ou un dossier.
    pub rules: Option<String>,
}

impl CollectionNode {
    /// Vrai pour une collection dynamique : ses photos sont calculées.
    #[must_use]
    pub fn is_smart(&self) -> bool {
        self.rules.is_some()
    }
}

/// Colonnes lues par [`read`].
const NODE_COLUMNS: &str = "collections.id, collections.parent_id, collections.kind,
    collections.name, smart_collections.rules
    FROM collections
    LEFT JOIN smart_collections ON smart_collections.collection_id = collections.id";

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectionNode> {
    let kind: String = row.get(2)?;
    Ok(CollectionNode {
        id: CollectionId::new(row.get(0)?),
        parent_id: row.get::<_, Option<i64>>(1)?.map(CollectionId::new),
        kind: if kind == "folder" {
            CollectionKind::Folder
        } else {
            CollectionKind::Collection
        },
        name: row.get(3)?,
        rules: row.get(4)?,
    })
}

/// Tout l'arbre, par ordre alphabétique.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn list(connection: &Connection) -> Result<Vec<CollectionNode>, CatalogError> {
    Ok(connection
        .prepare_cached(&format!(
            "SELECT {NODE_COLUMNS} ORDER BY collections.name_key"
        ))?
        .query_map([], read)?
        .collect::<Result<_, _>>()?)
}

/// Une collection ou un dossier ; `None` s'il n'existe pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn get(
    connection: &Connection,
    id: CollectionId,
) -> Result<Option<CollectionNode>, CatalogError> {
    Ok(connection
        .prepare_cached(&format!("SELECT {NODE_COLUMNS} WHERE collections.id = ?1"))?
        .query_row([id.get()], read)
        .optional()?)
}

/// Voisin de ce nom sous `parent` (sans tenir compte des majuscules).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_sibling(
    connection: &Connection,
    parent: Option<CollectionId>,
    name: &str,
) -> Result<Option<CollectionId>, CatalogError> {
    Ok(connection
        .prepare_cached(
            "SELECT id FROM collections WHERE coalesce(parent_id, 0) = coalesce(?1, 0)
                 AND name_key = ?2",
        )?
        .query_row(
            params![parent.map(CollectionId::get), keyword_name_key(name)],
            |row| row.get(0),
        )
        .optional()?
        .map(CollectionId::new))
}

/// Identifiant d'une nouvelle collection, choisi avant de la créer : une
/// annulation puis un rétablissement la recréent sous le même.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn next_id(connection: &Connection) -> Result<CollectionId, CatalogError> {
    let id: i64 = connection
        .prepare_cached("SELECT coalesce(max(id), 0) + 1 FROM collections")?
        .query_row([], |row| row.get(0))?;
    Ok(CollectionId::new(id))
}

/// Crée une collection ou un dossier, avec son identifiant.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite (nom pris, parent absent).
pub fn insert(connection: &Connection, node: &CollectionNode) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO collections (id, parent_id, kind, name, name_key)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?
        .execute(params![
            node.id.get(),
            node.parent_id.map(CollectionId::get),
            node.kind.as_str(),
            node.name,
            keyword_name_key(&node.name)
        ])?;
    set_rules(connection, node.id, node.rules.as_deref())
}

/// Règles d'une collection dynamique (`None` : elle n'en a pas).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_rules(
    connection: &Connection,
    id: CollectionId,
    rules: Option<&str>,
) -> Result<(), CatalogError> {
    match rules {
        Some(rules) => connection
            .prepare_cached(
                "INSERT INTO smart_collections (collection_id, rules) VALUES (?1, ?2)
                 ON CONFLICT (collection_id) DO UPDATE SET rules = excluded.rules",
            )?
            .execute(params![id.get(), rules])?,
        None => connection
            .prepare_cached("DELETE FROM smart_collections WHERE collection_id = ?1")?
            .execute([id.get()])?,
    };
    Ok(())
}

/// Rule chains of the smart collections that make up the content of `id`
/// (itself, or the collections of a set): one chain per smart collection
/// whose parent is not smart, each chain listing the rules to combine with
/// AND, from the topmost smart ancestor down. A nested smart collection
/// refines its parent, so it is already covered by its topmost ancestor's
/// chain, except for `id` itself, which brings its smart ancestors along.
///
/// # Errors
///
/// Fails if the database is unreadable.
pub fn smart_rule_chains(
    connection: &Connection,
    id: CollectionId,
) -> Result<Vec<Vec<String>>, CatalogError> {
    let nodes = branch(connection, id)?;
    let smart: std::collections::HashSet<CollectionId> = nodes
        .iter()
        .filter(|node| node.is_smart())
        .map(|node| node.id)
        .collect();
    let mut chains = Vec::new();
    for node in &nodes {
        let Some(rules) = &node.rules else {
            continue;
        };
        if node.id == id {
            let mut chain = smart_ancestor_rules(connection, node.parent_id)?;
            chain.push(rules.clone());
            chains.push(chain);
        } else if node.parent_id.is_none_or(|parent| !smart.contains(&parent)) {
            chains.push(vec![rules.clone()]);
        }
    }
    Ok(chains)
}

/// Rules of the smart ancestors starting at `parent`, from the topmost one
/// down to `parent`; empty when `parent` is not a smart collection.
fn smart_ancestor_rules(
    connection: &Connection,
    mut parent: Option<CollectionId>,
) -> Result<Vec<String>, CatalogError> {
    let mut rules = Vec::new();
    // At most as many steps as collections: protects against a loop.
    let mut steps = 0_usize;
    while let Some(id) = parent {
        let Some(node) = get(connection, id)? else {
            break;
        };
        let Some(node_rules) = node.rules else {
            break;
        };
        rules.push(node_rules);
        parent = node.parent_id;
        steps += 1;
        if steps > 10_000 {
            break;
        }
    }
    rules.reverse();
    Ok(rules)
}

/// Supprime une collection ou un dossier sans enfant (ses photos quittent la
/// collection, pas le catalogue).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite (le dossier a des enfants).
pub fn delete(connection: &Connection, id: CollectionId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM collections WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}

/// Change le nom.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite (nom déjà pris).
pub fn rename(connection: &Connection, id: CollectionId, name: &str) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE collections SET name = ?2, name_key = ?3 WHERE id = ?1")?
        .execute(params![id.get(), name, keyword_name_key(name)])?;
    Ok(())
}

/// Range sous un autre dossier (`None` : à la racine).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite (nom déjà pris).
pub fn reparent(
    connection: &Connection,
    id: CollectionId,
    parent: Option<CollectionId>,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE collections SET parent_id = ?2 WHERE id = ?1")?
        .execute(params![id.get(), parent.map(CollectionId::get)])?;
    Ok(())
}

/// Le nœud et tout ce qu'il contient, chaque parent avant ses enfants.
/// `UNION` arrête le parcours même en cas de boucle dans une base abîmée.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn branch(
    connection: &Connection,
    id: CollectionId,
) -> Result<Vec<CollectionNode>, CatalogError> {
    Ok(connection
        .prepare_cached(
            "WITH RECURSIVE branch(id, depth) AS (
                 SELECT ?1, 0
                 UNION
                 SELECT collections.id, branch.depth + 1
                 FROM collections JOIN branch ON collections.parent_id = branch.id
             )
             SELECT collections.id, collections.parent_id, collections.kind, collections.name,
                 smart_collections.rules
             FROM collections JOIN branch ON branch.id = collections.id
             LEFT JOIN smart_collections ON smart_collections.collection_id = collections.id
             ORDER BY branch.depth, collections.id",
        )?
        .query_map([id.get()], read)?
        .collect::<Result<_, _>>()?)
}

/// Nombre de photos de chaque collection et de chaque dossier (chaque
/// photo une seule fois dans un dossier, même présente dans plusieurs de
/// ses collections).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn media_counts(connection: &Connection) -> Result<Vec<(CollectionId, u64)>, CatalogError> {
    let rows = connection
        .prepare_cached(
            "WITH RECURSIVE ancestry(collection_id, ancestor_id) AS (
                 SELECT id, id FROM collections
                 UNION
                 SELECT ancestry.collection_id, collections.parent_id
                 FROM ancestry JOIN collections ON collections.id = ancestry.ancestor_id
                 WHERE collections.parent_id IS NOT NULL
             )
             SELECT ancestry.ancestor_id, count(DISTINCT collection_media.media_id)
             FROM ancestry
             JOIN collection_media ON collection_media.collection_id = ancestry.collection_id
             GROUP BY ancestry.ancestor_id",
        )?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, count)| Ok((CollectionId::new(id), size_from_sql(count)?)))
        .collect()
}

/// Photos d'une collection.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn media_of(connection: &Connection, id: CollectionId) -> Result<Vec<MediaId>, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT media_id FROM collection_media WHERE collection_id = ?1")?
        .query_map([id.get()], |row| row.get(0).map(MediaId::new))?
        .collect::<Result<_, _>>()?)
}

/// Parmi `ids`, les photos déjà dans la collection.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn present(
    connection: &Connection,
    id: CollectionId,
    ids: &[MediaId],
) -> Result<Vec<MediaId>, CatalogError> {
    let mut found = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media_id FROM collection_media WHERE collection_id = ?1
                 AND media_id IN ({})",
            placeholders(2, chunk.len())
        );
        let values = std::iter::once(id.get()).chain(chunk.iter().map(|media| media.get()));
        found.extend(
            connection
                .prepare_cached(&sql)?
                .query_map(params_from_iter(values), |row| row.get(0).map(MediaId::new))?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(found)
}

/// Photos qui existent au catalogue, parmi `ids`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn existing_media(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<MediaId>, CatalogError> {
    let mut found = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id FROM media WHERE id IN ({})",
            placeholders(1, chunk.len())
        );
        found.extend(
            connection
                .prepare_cached(&sql)?
                .query_map(params_from_iter(chunk.iter().map(|id| id.get())), |row| {
                    row.get(0).map(MediaId::new)
                })?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(found)
}

/// Met ces photos dans la collection (déjà présentes : sans effet).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn add(connection: &Connection, id: CollectionId, ids: &[MediaId]) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "INSERT OR IGNORE INTO collection_media (collection_id, media_id) VALUES (?1, ?2)",
    )?;
    for media in ids {
        statement.execute([id.get(), media.get()])?;
    }
    Ok(())
}

/// Retire ces photos de la collection (elles restent au catalogue).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn remove(
    connection: &Connection,
    id: CollectionId,
    ids: &[MediaId],
) -> Result<(), CatalogError> {
    let mut statement = connection.prepare_cached(
        "DELETE FROM collection_media WHERE collection_id = ?1 AND media_id = ?2",
    )?;
    for media in ids {
        statement.execute([id.get(), media.get()])?;
    }
    Ok(())
}
