//! Saisie des mots-clés : du texte tapé aux mots-clés de l'arbre, et
//! suggestions de l'autocomplétion.

use std::collections::HashMap;

use rusqlite::Connection;

use super::{KeywordTree, MAX_SUGGESTIONS};
use crate::catalog::CatalogError;
use crate::catalog::repositories::keywords::{self, FirstLevel, Keyword, SuggestionScope};
use crate::domain::{KeywordEntryError, KeywordId, keyword_name_key, parse_keyword_entry};

/// Saisie qui n'a pas pu être appliquée.
#[derive(Debug, thiserror::Error)]
pub enum KeywordError {
    /// La saisie est mal formée (nom trop long, caractère interdit).
    #[error(transparent)]
    Entry(#[from] KeywordEntryError),
    /// Un nom seul désigne plusieurs mots-clés, aucun à la racine : il faut
    /// choisir une suggestion ou taper son chemin.
    #[error("keyword {0:?} exists in several places")]
    Ambiguous(String),
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Mots-clés désignés par une saisie comme `Lieux > France > Paris,
/// Vacances`, créés s'ils n'existent pas encore. Le premier
/// niveau d'un chemin est cherché comme un nom seul : à la racine, sinon le
/// seul qui porte ce nom ; les niveaux suivants parmi les enfants.
///
/// À appeler dans une transaction : en cas d'erreur, aucun mot-clé ne doit
/// rester créé.
///
/// # Errors
///
/// Voir [`KeywordError`].
pub fn resolve_entry(connection: &Connection, entry: &str) -> Result<Vec<KeywordId>, KeywordError> {
    let mut resolved = Vec::new();
    for path in parse_keyword_entry(entry)? {
        let mut current: Option<KeywordId> = None;
        for (level, name) in path.iter().enumerate() {
            let existing = if level == 0 {
                match keywords::first_level(connection, name)? {
                    FirstLevel::Found(id) => Some(id),
                    FirstLevel::Missing => None,
                    FirstLevel::Ambiguous => return Err(KeywordError::Ambiguous(name.clone())),
                }
            } else {
                keywords::child(connection, current, name)?
            };
            current = Some(match existing {
                Some(id) => id,
                None => keywords::create(connection, current, name)?,
            });
        }
        if let Some(id) = current
            && !resolved.contains(&id)
        {
            resolved.push(id);
        }
    }
    Ok(resolved)
}

/// Suggestion de l'autocomplétion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Mot-clé proposé.
    pub id: KeywordId,
    /// Son nom.
    pub name: String,
    /// Noms de ses parents, du plus haut au plus proche.
    pub parents: Vec<String>,
    /// Vrai si un autre mot-clé porte le même nom : le chemin est alors
    /// affiché pour les distinguer.
    pub ambiguous: bool,
}

/// Suggestions pour le mot-clé en cours de saisie (`token` : ce qui suit la
/// dernière virgule, par exemple `Lieux > Pays-Bas > Ze`). Après un « > »,
/// seuls les enfants du parent tapé sont proposés ; rien tant que la
/// dernière partie est vide, ou si le parent tapé n'existe pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn suggestions(connection: &Connection, token: &str) -> Result<Vec<Suggestion>, CatalogError> {
    let mut levels: Vec<&str> = token.split('>').map(str::trim).collect();
    let search = levels.pop().unwrap_or_default();
    if search.is_empty() {
        return Ok(Vec::new());
    }
    let scope = match parent_of(connection, &levels)? {
        Some(parent) => SuggestionScope::ChildrenOf(parent),
        None if levels.iter().all(|level| level.is_empty()) => SuggestionScope::Anywhere,
        None => return Ok(Vec::new()),
    };
    let found = keywords::suggest(connection, scope, search, MAX_SUGGESTIONS)?;
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let tree = Tree::new(keywords::all(connection)?);
    Ok(found
        .into_iter()
        .filter_map(|(id, _)| tree.suggestion(id))
        .collect())
}

/// Mot-clé désigné par les niveaux déjà tapés, sans rien créer ; `None` si
/// aucun niveau n'est tapé, ou s'il n'existe pas ou est ambigu.
fn parent_of(connection: &Connection, levels: &[&str]) -> Result<Option<KeywordId>, CatalogError> {
    let mut current: Option<KeywordId> = None;
    for (level, name) in levels.iter().filter(|name| !name.is_empty()).enumerate() {
        current = if level == 0 {
            match keywords::first_level(connection, name)? {
                FirstLevel::Found(id) => Some(id),
                FirstLevel::Missing | FirstLevel::Ambiguous => return Ok(None),
            }
        } else {
            match keywords::child(connection, current, name)? {
                Some(id) => Some(id),
                None => return Ok(None),
            }
        };
    }
    Ok(current)
}

/// Arbre des mots-clés, avec le nombre de mots-clés de chaque nom : un nom
/// porté par plusieurs mots-clés est affiché avec son chemin.
struct Tree {
    tree: KeywordTree,
    /// Nombre de mots-clés par nom (clé d'identité).
    name_counts: HashMap<String, usize>,
}

impl Tree {
    fn new(all: Vec<Keyword>) -> Self {
        let mut name_counts = HashMap::new();
        for keyword in &all {
            *name_counts
                .entry(keyword_name_key(&keyword.name))
                .or_insert(0) += 1;
        }
        Self {
            tree: KeywordTree::new(all),
            name_counts,
        }
    }

    fn suggestion(&self, id: KeywordId) -> Option<Suggestion> {
        let keyword = self.tree.get(id)?;
        Some(Suggestion {
            id,
            name: keyword.name.clone(),
            parents: self.tree.parent_names(id),
            ambiguous: self
                .name_counts
                .get(&keyword_name_key(&keyword.name))
                .is_some_and(|count| *count > 1),
        })
    }
}
