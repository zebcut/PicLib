//! Filtre mémorisé pour chaque source de la grille : son
//! contenu JSON, que les commandes vérifient à la lecture.

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::domain::{CollectionId, FolderId, KeywordId};

/// Source dont le filtre est mémorisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSource {
    /// Tout le catalogue.
    All,
    /// Le dernier import, quel qu'il soit.
    LatestImport,
    /// Un mot-clé.
    Keyword(KeywordId),
    /// Un dossier.
    Folder(FolderId),
    /// Une collection ou un dossier de collections.
    Collection(CollectionId),
    /// La collection rapide.
    QuickCollection,
    /// Les photos manquantes.
    Missing,
}

impl FilterSource {
    /// Colonnes `kind`, `keyword_id`, `folder_id` et `collection_id` de
    /// la source.
    fn columns(self) -> (&'static str, Option<i64>, Option<i64>, Option<i64>) {
        match self {
            Self::All => ("all", None, None, None),
            Self::LatestImport => ("latestImport", None, None, None),
            Self::Keyword(id) => ("keyword", Some(id.get()), None, None),
            Self::Folder(id) => ("folder", None, Some(id.get()), None),
            Self::Collection(id) => ("collection", None, None, Some(id.get())),
            Self::QuickCollection => ("quickCollection", None, None, None),
            Self::Missing => ("missing", None, None, None),
        }
    }
}

/// Filtre mémorisé d'une source ; `None` s'il n'y en a pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn get(connection: &Connection, source: FilterSource) -> Result<Option<String>, CatalogError> {
    let (kind, keyword, folder, collection) = source.columns();
    Ok(connection
        .prepare_cached(
            "SELECT content FROM source_filters
             WHERE kind = ?1 AND coalesce(keyword_id, 0) = coalesce(?2, 0)
                 AND coalesce(folder_id, 0) = coalesce(?3, 0)
                 AND coalesce(collection_id, 0) = coalesce(?4, 0)",
        )?
        .query_row(params![kind, keyword, folder, collection], |row| row.get(0))
        .optional()?)
}

/// Mémorise le filtre d'une source (`None` : l'oublie). Sans effet pour un
/// mot-clé ou un dossier qui n'est plus au catalogue.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn put(
    connection: &Connection,
    source: FilterSource,
    content: Option<&str>,
) -> Result<(), CatalogError> {
    let (kind, keyword, folder, collection) = source.columns();
    connection
        .prepare_cached(
            "DELETE FROM source_filters
             WHERE kind = ?1 AND coalesce(keyword_id, 0) = coalesce(?2, 0)
                 AND coalesce(folder_id, 0) = coalesce(?3, 0)
                 AND coalesce(collection_id, 0) = coalesce(?4, 0)",
        )?
        .execute(params![kind, keyword, folder, collection])?;
    if let Some(content) = content {
        connection
            .prepare_cached(
                "INSERT INTO source_filters (kind, keyword_id, folder_id, collection_id, content)
                 SELECT ?1, ?2, ?3, ?4, ?5
                 WHERE (?2 IS NULL OR EXISTS (SELECT 1 FROM keywords WHERE id = ?2))
                     AND (?3 IS NULL OR EXISTS (SELECT 1 FROM folders WHERE id = ?3))
                     AND (?4 IS NULL OR EXISTS (SELECT 1 FROM collections WHERE id = ?4))",
            )?
            .execute(params![kind, keyword, folder, collection, content])?;
    }
    Ok(())
}
