//! Mots-clés trouvés dans les fichiers à l'import : du
//! chemin lu aux mots-clés de l'arbre, créés s'il le faut.

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::keywords::{self, FirstLevel};
use crate::domain::KeywordId;
use crate::metadata::KeywordPath;

/// Mots-clés désignés par des chemins lus dans un fichier, sans doublon :
///
/// - un chemin complet (`Lieux|France|Paris`) part de la racine, comme dans
///   Lightroom ; les niveaux manquants sont créés ;
/// - un nom seul désigne le mot-clé de la racine s'il existe, sinon le seul
///   qui porte ce nom ailleurs ; s'il n'y en a aucun, ou plusieurs, il est
///   créé à la racine.
///
/// À appeler dans la transaction qui pose ces mots-clés.
///
/// # Errors
///
/// Échoue si la base ne peut pas être lue ou écrite.
pub fn resolve_recovered(
    connection: &Connection,
    paths: &[KeywordPath],
) -> Result<Vec<KeywordId>, CatalogError> {
    let mut resolved = Vec::with_capacity(paths.len());
    for path in paths {
        let id = match path {
            KeywordPath::FromRoot(names) => from_root(connection, names)?,
            KeywordPath::Name(name) => match keywords::first_level(connection, name)? {
                FirstLevel::Found(id) => Some(id),
                FirstLevel::Missing | FirstLevel::Ambiguous => {
                    from_root(connection, std::slice::from_ref(name))?
                }
            },
        };
        if let Some(id) = id
            && !resolved.contains(&id)
        {
            resolved.push(id);
        }
    }
    Ok(resolved)
}

/// Dernier mot-clé d'un chemin partant de la racine, niveaux créés au
/// besoin ; `None` pour un chemin vide.
fn from_root(connection: &Connection, names: &[String]) -> Result<Option<KeywordId>, CatalogError> {
    let mut current = None;
    for name in names {
        current = Some(match keywords::child(connection, current, name)? {
            Some(id) => id,
            None => keywords::create(connection, current, name)?,
        });
    }
    Ok(current)
}
