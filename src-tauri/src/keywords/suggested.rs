//! Suggestions de mots-clés : neuf mots-clés probables pour la
//! photo active, à poser d'un clic. (L'autocomplétion de la saisie, elle,
//! est dans `entry`.)
//!
//! Trois sources, dans cet ordre de confiance :
//! 1. les mots-clés des photos prises au même moment (dans l'heure, ou le
//!    même jour si l'heure ne donne rien) : une série partage souvent son
//!    lieu et son sujet ;
//! 2. les mots-clés qui accompagnent souvent ceux déjà posés sur la photo ;
//! 3. les mots-clés posés récemment.
//!
//! Dans chaque source, les plus fréquents d'abord. Un mot-clé déjà sur la
//! photo, ou parent d'un mot-clé qu'elle porte, n'est jamais proposé.

use std::collections::HashSet;

use rusqlite::Connection;

use super::KeywordTree;
use crate::catalog::CatalogError;
use crate::catalog::repositories::keyword_usage::{self, TimeWindow};
use crate::catalog::repositories::keywords;
use crate::domain::{KeywordId, MediaId, keyword_name_key};

/// Nombre de suggestions affichées (trois rangées de trois).
pub const MAX_SUGGESTED_KEYWORDS: usize = 9;

/// Mots-clés récents examinés pour compléter les suggestions.
const RECENT_CANDIDATES: usize = 30;

/// Suggestions pour la photo active `active` ; sans photo active, les
/// mots-clés récents seulement.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn suggested_keywords(
    connection: &Connection,
    active: Option<MediaId>,
) -> Result<Vec<KeywordId>, CatalogError> {
    let tree = KeywordTree::new(keywords::all(connection)?);
    let mut picked = Picked::new(&tree);
    if let Some(media) = active {
        for (_, keyword) in keywords::links(connection, &[media])? {
            picked.exclude_with_parents(keyword);
        }
        let mut near = picked.ranked(keyword_usage::near_in_time(
            connection,
            media,
            TimeWindow::Hour,
        )?);
        if near.is_empty() {
            near = picked.ranked(keyword_usage::near_in_time(
                connection,
                media,
                TimeWindow::Day,
            )?);
        }
        picked.extend(near);
        let related = picked.ranked(keyword_usage::co_occurring(connection, media)?);
        picked.extend(related);
    }
    picked.extend(keyword_usage::recently_used(connection, RECENT_CANDIDATES)?);
    Ok(picked.chosen)
}

/// Suggestions retenues, et mots-clés à ne plus proposer.
struct Picked<'a> {
    tree: &'a KeywordTree,
    excluded: HashSet<KeywordId>,
    chosen: Vec<KeywordId>,
}

impl<'a> Picked<'a> {
    fn new(tree: &'a KeywordTree) -> Self {
        Self {
            tree,
            excluded: HashSet::new(),
            chosen: Vec::with_capacity(MAX_SUGGESTED_KEYWORDS),
        }
    }

    /// Écarte un mot-clé de la photo, et ses parents (déjà impliqués).
    fn exclude_with_parents(&mut self, keyword: KeywordId) {
        self.excluded.extend(self.tree.lineage(keyword));
    }

    /// Candidats d'une source encore proposables, les plus fréquents
    /// d'abord (à égalité, dans l'ordre alphabétique, pour un résultat
    /// stable).
    fn ranked(&self, counts: Vec<(KeywordId, u32)>) -> Vec<KeywordId> {
        let mut candidates: Vec<(KeywordId, u32, String)> = counts
            .into_iter()
            .filter(|(id, _)| self.is_candidate(*id))
            .filter_map(|(id, count)| {
                let keyword = self.tree.get(id)?;
                Some((id, count, keyword_name_key(&keyword.name)))
            })
            .collect();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        candidates.into_iter().map(|(id, _, _)| id).collect()
    }

    /// Ajoute des candidats, dans l'ordre, jusqu'à neuf suggestions.
    fn extend(&mut self, candidates: impl IntoIterator<Item = KeywordId>) {
        for id in candidates {
            if self.chosen.len() == MAX_SUGGESTED_KEYWORDS {
                return;
            }
            if self.is_candidate(id) {
                self.excluded.insert(id);
                self.chosen.push(id);
            }
        }
    }

    fn is_candidate(&self, id: KeywordId) -> bool {
        !self.excluded.contains(&id) && self.tree.get(id).is_some()
    }
}
