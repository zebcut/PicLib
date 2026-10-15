//! Arbre des mots-clés en mémoire (quelques milliers au plus) : parents,
//! enfants et branches, protégés contre une boucle dans une base abîmée.

use std::collections::{HashMap, HashSet};

use crate::catalog::repositories::keywords::Keyword;
use crate::domain::{KeywordId, keyword_name_key};

/// Arbre des mots-clés, lu d'un coup dans le catalogue.
#[derive(Debug, Clone, Default)]
pub struct KeywordTree {
    by_id: HashMap<KeywordId, Keyword>,
    /// Enfants de chaque mot-clé (`None` : la racine), dans l'ordre de la
    /// base.
    children: HashMap<Option<KeywordId>, Vec<KeywordId>>,
}

impl KeywordTree {
    /// Construit l'arbre à partir de tous les mots-clés du catalogue.
    #[must_use]
    pub fn new(all: Vec<Keyword>) -> Self {
        let mut children: HashMap<Option<KeywordId>, Vec<KeywordId>> = HashMap::new();
        for keyword in &all {
            children
                .entry(keyword.parent_id)
                .or_default()
                .push(keyword.id);
        }
        Self {
            by_id: all
                .into_iter()
                .map(|keyword| (keyword.id, keyword))
                .collect(),
            children,
        }
    }

    /// Le mot-clé `id`, s'il existe.
    #[must_use]
    pub fn get(&self, id: KeywordId) -> Option<&Keyword> {
        self.by_id.get(&id)
    }

    /// Nombre de mots-clés.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Vrai s'il n'y a aucun mot-clé.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Tous les mots-clés, sans ordre particulier.
    pub fn keywords(&self) -> impl Iterator<Item = &Keyword> {
        self.by_id.values()
    }

    /// Enfants directs de `parent` (`None` : les mots-clés de la racine).
    #[must_use]
    pub fn children(&self, parent: Option<KeywordId>) -> &[KeywordId] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// Enfant de `parent` qui porte ce nom (majuscules ignorées), s'il
    /// existe.
    #[must_use]
    pub fn child_named(&self, parent: Option<KeywordId>, name: &str) -> Option<KeywordId> {
        let key = keyword_name_key(name);
        self.children(parent).iter().copied().find(|id| {
            self.get(*id)
                .is_some_and(|keyword| keyword_name_key(&keyword.name) == key)
        })
    }

    /// Le mot-clé puis ses parents, jusqu'à la racine. S'arrête sur un
    /// parent inconnu ou après autant d'étapes que de mots-clés (boucle dans
    /// une base abîmée).
    pub fn lineage(&self, start: KeywordId) -> impl Iterator<Item = KeywordId> + '_ {
        std::iter::successors(Some(start), |id| {
            self.get(*id).and_then(|keyword| keyword.parent_id)
        })
        .take(self.len().max(1))
    }

    /// Noms des parents d'un mot-clé, du plus haut au plus proche.
    #[must_use]
    pub fn parent_names(&self, id: KeywordId) -> Vec<String> {
        let mut names: Vec<String> = self
            .lineage(id)
            .skip(1)
            .filter_map(|ancestor| self.get(ancestor).map(|keyword| keyword.name.clone()))
            .collect();
        names.reverse();
        names
    }

    /// Vrai si `id` est `ancestor` ou se trouve dans sa branche.
    #[must_use]
    pub fn is_within(&self, id: KeywordId, ancestor: KeywordId) -> bool {
        self.lineage(id).any(|candidate| candidate == ancestor)
    }

    /// Le mot-clé et tous ses descendants, chaque parent avant ses enfants.
    /// Vide si le mot-clé n'existe pas.
    #[must_use]
    pub fn branch(&self, root: KeywordId) -> Vec<KeywordId> {
        if self.get(root).is_none() {
            return Vec::new();
        }
        let mut branch = Vec::new();
        let mut seen = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            // Un mot-clé déjà vu signale une boucle : on ne le reprend pas.
            if !seen.insert(id) {
                continue;
            }
            branch.push(id);
            pending.extend(self.children(Some(id)).iter().rev());
        }
        branch
    }
}
