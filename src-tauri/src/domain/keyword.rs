//! Mots-clés hiérarchiques : lecture d'une saisie et clés de
//! comparaison des noms.

use unicode_normalization::UnicodeNormalization;

/// Longueur maximale d'un nom de mot-clé, en caractères.
pub const MAX_KEYWORD_CHARS: usize = 200;

/// Caractères interdits dans un nom : la virgule et « > » séparent les
/// mots-clés et les niveaux à la saisie, « | » sépare les niveaux dans le
/// champ hiérarchique XMP de Lightroom.
const FORBIDDEN: [char; 3] = [',', '>', '|'];

/// Saisie refusée.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeywordEntryError {
    /// Un nom dépasse [`MAX_KEYWORD_CHARS`] caractères.
    #[error("keyword name too long: {0:?}")]
    TooLong(String),
    /// Un nom contient « | » (ou, pour un nom seul, « , » ou « > »).
    #[error("keyword name contains a forbidden character: {0:?}")]
    ForbiddenCharacter(String),
}

/// Lit une saisie comme `Lieux > France > Paris, Vacances, Été` : une liste
/// de chemins (du niveau le plus haut au mot-clé à appliquer). La virgule
/// sépare les mots-clés, « > » les niveaux ; les espaces autour et les
/// éléments vides sont ignorés.
///
/// # Errors
///
/// Voir [`KeywordEntryError`].
pub fn parse_keyword_entry(entry: &str) -> Result<Vec<Vec<String>>, KeywordEntryError> {
    let mut paths = Vec::new();
    for keyword in entry.split(',') {
        let mut path = Vec::new();
        for level in keyword.split('>') {
            let name: String = level.trim().nfc().collect();
            if name.is_empty() {
                continue;
            }
            if name.contains(FORBIDDEN) {
                return Err(KeywordEntryError::ForbiddenCharacter(name));
            }
            if name.chars().count() > MAX_KEYWORD_CHARS {
                return Err(KeywordEntryError::TooLong(name));
            }
            path.push(name);
        }
        if !path.is_empty() {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Lit le nom d'un seul mot-clé (création, renommage dans la liste) :
/// espaces autour ignorés, forme Unicode composée ; `None` s'il est vide.
/// Ici, « , » et « > » sont refusés comme « | » : ils ne séparent rien.
///
/// # Errors
///
/// Voir [`KeywordEntryError`].
pub fn parse_keyword_name(name: &str) -> Result<Option<String>, KeywordEntryError> {
    let name: String = name.trim().nfc().collect();
    if name.is_empty() {
        return Ok(None);
    }
    if name.contains(FORBIDDEN) {
        return Err(KeywordEntryError::ForbiddenCharacter(name));
    }
    if name.chars().count() > MAX_KEYWORD_CHARS {
        return Err(KeywordEntryError::TooLong(name));
    }
    Ok(Some(name))
}

/// Nom d'un mot-clé lu dans un fichier : espaces autour ignorés,
/// forme Unicode composée ; les caractères interdits sont remplacés (« , » par « ; », « > » et « | » par « - ») pour garder le
/// mot-clé. `None` s'il est vide ou trop long.
#[must_use]
pub fn recovered_keyword_name(name: &str) -> Option<String> {
    let name: String = name
        .trim()
        .nfc()
        .map(|c| match c {
            ',' => ';',
            '>' | '|' => '-',
            other => other,
        })
        .collect();
    (!name.is_empty() && name.chars().count() <= MAX_KEYWORD_CHARS).then_some(name)
}

/// Clé d'identité d'un nom : forme Unicode composée et minuscules.
/// « Paris » et « paris » sont le même mot-clé ; « Été » et « Ete » non.
#[must_use]
pub fn keyword_name_key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// Clé de recherche d'un nom, pour l'autocomplétion : sans majuscules ni
/// accents (« ete » trouve « Été »).
#[must_use]
pub fn keyword_search_key(name: &str) -> String {
    super::search_key(name)
}
