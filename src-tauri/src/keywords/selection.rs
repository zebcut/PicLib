//! Mots-clés d'une sélection, pour le panneau « Mots-clés » et
//! les cases à cocher de la liste des mots-clés.

use std::collections::{HashMap, HashSet};

use super::KeywordTree;
use crate::domain::{KeywordId, MediaId, keyword_search_key};

/// Présence d'un mot-clé saisi sur les photos sélectionnées : la case de
/// la liste des mots-clés est vide, marquée d'un tiret ou cochée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Saisi sur aucune photo (seulement parent d'un mot-clé saisi).
    Absent,
    /// Saisi sur une partie des photos.
    Partial,
    /// Saisi sur toutes les photos.
    All,
}

/// Un mot-clé affiché pour la sélection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionKeyword {
    /// Mot-clé.
    pub id: KeywordId,
    /// Son parent (`None` : à la racine), pour l'afficher en arbre.
    pub parent_id: Option<KeywordId>,
    /// Son nom.
    pub name: String,
    /// Noms de ses parents, du plus haut au plus proche.
    pub parents: Vec<String>,
    /// Vrai s'il concerne toutes les photos sélectionnées, lui-même ou par
    /// un de ses descendants (sinon il est marqué d'une étoile, comme dans
    /// Lightroom).
    pub on_all: bool,
    /// Présence du mot-clé saisi lui-même : s'il est saisi sur au moins une
    /// photo, il peut être retiré.
    pub entered: Presence,
}

/// Les deux listes du panneau.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionKeywords {
    /// Mots-clés saisis et leurs parents(« parents impliqués ») :
    /// de quoi afficher l'arbre.
    pub with_parents: Vec<SelectionKeyword>,
    /// Mots-clés qui seront exportés : saisis et parents, sauf ceux dont
    /// « inclure à l'export » est décoché.
    pub exported: Vec<SelectionKeyword>,
}

/// Calcule les listes à partir de l'arbre des mots-clés et des liens
/// (média, mot-clé saisi) des `selected_count` médias sélectionnés.
#[must_use]
pub fn selection_keywords(
    tree: &KeywordTree,
    links: &[(MediaId, KeywordId)],
    selected_count: usize,
) -> SelectionKeywords {
    let mut entered: HashMap<KeywordId, usize> = HashMap::new();
    // Médias concernés par chaque mot-clé, lui-même ou un de ses
    // descendants.
    let mut covered: HashMap<KeywordId, HashSet<MediaId>> = HashMap::new();
    for &(media, keyword) in links {
        *entered.entry(keyword).or_insert(0) += 1;
        for id in tree.lineage(keyword) {
            covered.entry(id).or_default().insert(media);
        }
    }

    let mut with_parents: Vec<SelectionKeyword> = covered
        .iter()
        .filter_map(|(&id, media)| {
            let keyword = tree.get(id)?;
            Some(SelectionKeyword {
                id,
                parent_id: keyword.parent_id,
                name: keyword.name.clone(),
                parents: tree.parent_names(id),
                on_all: media.len() >= selected_count,
                entered: match entered.get(&id) {
                    None => Presence::Absent,
                    Some(&count) if count >= selected_count => Presence::All,
                    Some(_) => Presence::Partial,
                },
            })
        })
        .collect();
    with_parents.sort_by_cached_key(|keyword| {
        (keyword_search_key(&keyword.name), keyword.parents.join(">"))
    });
    let exported = with_parents
        .iter()
        .filter(|keyword| {
            tree.get(keyword.id)
                .is_some_and(|keyword| keyword.include_on_export)
        })
        .cloned()
        .collect();
    SelectionKeywords {
        with_parents,
        exported,
    }
}
