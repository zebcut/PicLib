//! Format texte de la liste des mots-clés de Lightroom : un
//! mot-clé par ligne, une tabulation de plus par niveau de l'arbre,
//! `[Lieux]` pour un mot-clé qui n'est pas inclus à l'export, `{Synonyme}`
//! sur la ligne suivante (décalé d'un niveau) pour un synonyme.
//!
//! Fonctions pures, sans base ni disque : `transfer` les relie au
//! catalogue et au module `files`.

use std::collections::HashMap;

use super::KeywordTree;
use crate::domain::{
    KeywordEntryError, KeywordId, keyword_name_key, keyword_search_key, parse_keyword_name,
};

/// Marque d'ordre des octets qu'ajoutent certains éditeurs en tête d'un
/// fichier UTF-8.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// Un mot-clé lu dans le fichier, placé après son parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextKeyword {
    /// Position de son parent dans la liste lue ; `None` à la racine.
    pub parent: Option<usize>,
    /// Nom (espaces autour retirés, forme Unicode composée).
    pub name: String,
    /// Faux pour un nom entre crochets (`[Lieux]`).
    pub include_on_export: bool,
    /// Numéro de la première ligne qui le décrit (à partir de 1).
    pub line: usize,
}

/// Ligne ignorée, et pourquoi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIssue {
    /// Numéro de la ligne (à partir de 1).
    pub line: usize,
    /// Raison.
    pub reason: LineIssueReason,
}

/// Raison pour laquelle une ligne est ignorée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineIssueReason {
    /// Décalée de plusieurs niveaux d'un coup par rapport à la ligne du
    /// dessus : on ne sait pas sous quel mot-clé la ranger.
    TooDeep,
    /// Son parent a été ignoré : elle l'est aussi.
    ParentIgnored,
    /// Nom vide (`[]`, par exemple).
    EmptyName,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Nom contenant « , », « > » ou « | ».
    ForbiddenCharacter(String),
}

/// Contenu d'un fichier de mots-clés.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextKeywordList {
    /// Mots-clés, chaque parent avant ses enfants ; un même mot-clé écrit
    /// deux fois au même endroit n'y figure qu'une fois.
    pub keywords: Vec<TextKeyword>,
    /// Nombre de synonymes lus (ils ne sont pas gérés en version 1).
    pub synonyms: usize,
    /// Lignes ignorées.
    pub issues: Vec<LineIssue>,
}

/// Lit le texte d'un fichier de mots-clés. Tolère les fins de ligne de
/// Windows et des anciens Mac, une marque d'ordre des octets en tête, les
/// lignes vides et les espaces ou tabulations en fin de ligne.
#[must_use]
pub fn parse_keyword_list(text: &str) -> TextKeywordList {
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let mut list = TextKeywordList::default();
    // Mot-clé retenu à chaque niveau, de la racine à la ligne précédente ;
    // `None` pour une ligne ignorée (ses enfants le seront aussi).
    let mut stack: Vec<Option<usize>> = Vec::new();
    // Mots-clés déjà lus, par parent et clé de nom : un doublon est fusionné.
    let mut known: HashMap<(Option<usize>, String), usize> = HashMap::new();

    // Fins de ligne de Windows (`\r\n`) et des anciens Mac (`\r`) ramenées
    // à `\n`, pour que les numéros de ligne signalés soient justes.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for (index, raw) in text.split('\n').enumerate() {
        let line = index + 1;
        let trimmed = raw.trim_end();
        let content = trimmed.trim_start_matches('\t');
        if content.trim().is_empty() {
            continue;
        }
        let depth = trimmed.len() - content.len();
        let content = content.trim();

        if let Some(synonym) = bracketed(content, '{', '}') {
            // Un synonyme ne change pas l'arbre ; il n'est pas géré en
            // version 1, mais ne doit pas devenir un mot-clé.
            if !synonym.trim().is_empty() {
                list.synonyms += 1;
            }
            continue;
        }
        if depth > stack.len() {
            list.issues.push(LineIssue {
                line,
                reason: LineIssueReason::TooDeep,
            });
            continue;
        }
        stack.truncate(depth);
        let parent = match stack.last() {
            None => None,
            Some(Some(parent)) => Some(*parent),
            Some(None) => {
                list.issues.push(LineIssue {
                    line,
                    reason: LineIssueReason::ParentIgnored,
                });
                stack.push(None);
                continue;
            }
        };
        let (name, include_on_export) = match bracketed(content, '[', ']') {
            Some(name) => (name, false),
            None => (content, true),
        };
        let name = match parse_keyword_name(name) {
            Ok(Some(name)) => name,
            Ok(None) => {
                list.issues.push(LineIssue {
                    line,
                    reason: LineIssueReason::EmptyName,
                });
                stack.push(None);
                continue;
            }
            Err(error) => {
                list.issues.push(LineIssue {
                    line,
                    reason: match error {
                        KeywordEntryError::TooLong(_) => LineIssueReason::TooLong,
                        KeywordEntryError::ForbiddenCharacter(name) => {
                            LineIssueReason::ForbiddenCharacter(name)
                        }
                    },
                });
                stack.push(None);
                continue;
            }
        };
        let position = *known
            .entry((parent, keyword_name_key(&name)))
            .or_insert_with(|| {
                list.keywords.push(TextKeyword {
                    parent,
                    name,
                    include_on_export,
                    line,
                });
                list.keywords.len() - 1
            });
        stack.push(Some(position));
    }
    list
}

/// Écrit tout l'arbre au format de Lightroom, chaque niveau trié comme dans
/// la liste des mots-clés (sans tenir compte des accents ni des
/// majuscules). `line_ending` : `"\n"` sur macOS, `"\r\n"` sous Windows.
#[must_use]
pub fn write_keyword_list(tree: &KeywordTree, line_ending: &str) -> String {
    let mut text = String::new();
    // Pile explicite plutôt que récursion : l'arbre d'une base abîmée ne
    // peut pas faire déborder la pile d'appels.
    let mut pending: Vec<(KeywordId, usize)> = sorted_children(tree, None)
        .into_iter()
        .rev()
        .map(|id| (id, 0))
        .collect();
    let mut written = 0;
    while let Some((id, depth)) = pending.pop() {
        let Some(keyword) = tree.get(id) else {
            continue;
        };
        // Au plus autant de lignes que de mots-clés : protège d'une boucle.
        written += 1;
        if written > tree.len() {
            break;
        }
        text.extend(std::iter::repeat_n('\t', depth));
        if keyword.include_on_export {
            text.push_str(&keyword.name);
        } else {
            text.push('[');
            text.push_str(&keyword.name);
            text.push(']');
        }
        text.push_str(line_ending);
        pending.extend(
            sorted_children(tree, Some(id))
                .into_iter()
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
    text
}

fn sorted_children(tree: &KeywordTree, parent: Option<KeywordId>) -> Vec<KeywordId> {
    let mut children: Vec<(String, String, KeywordId)> = tree
        .children(parent)
        .iter()
        .filter_map(|&id| {
            let keyword = tree.get(id)?;
            Some((keyword_search_key(&keyword.name), keyword.name.clone(), id))
        })
        .collect();
    children.sort();
    children.into_iter().map(|(_, _, id)| id).collect()
}

/// Contenu entre `open` et `close` si `text` commence et finit par eux.
fn bracketed(text: &str, open: char, close: char) -> Option<&str> {
    text.strip_prefix(open)?.strip_suffix(close)
}
