//! Nombre de photos de chaque mot-clé, pour la liste des mots-clés
//! : celles de toute sa branche, chacune comptée une fois.

use std::collections::{HashMap, HashSet};

use super::KeywordTree;
use crate::domain::{KeywordId, MediaId};

/// Nombre de photos de chaque mot-clé : celles qui portent le mot-clé ou un
/// de ses descendants, une photo n'étant comptée qu'une fois (« Lieux »
/// compte les photos de Paris et de Lyon). Les mots-clés sans photo sont
/// absents du résultat.
///
/// `links` doit être trié par média (l'ordre de la table `media_keywords`) :
/// les mots-clés d'une photo sont alors lus d'un bloc.
#[must_use]
pub fn branch_photo_counts(
    tree: &KeywordTree,
    links: impl IntoIterator<Item = (MediaId, KeywordId)>,
) -> HashMap<KeywordId, u64> {
    let mut counts: HashMap<KeywordId, u64> = HashMap::new();
    // Lignée de chaque mot-clé, calculée une seule fois.
    let mut lineages: HashMap<KeywordId, Vec<KeywordId>> = HashMap::new();
    // Mots-clés (avec leurs parents) de la photo en cours.
    let mut current: Option<MediaId> = None;
    let mut reached: HashSet<KeywordId> = HashSet::new();

    let mut flush = |reached: &mut HashSet<KeywordId>| {
        for id in reached.drain() {
            *counts.entry(id).or_insert(0) += 1;
        }
    };
    for (media, keyword) in links {
        if current != Some(media) {
            flush(&mut reached);
            current = Some(media);
        }
        let lineage = lineages
            .entry(keyword)
            .or_insert_with(|| tree.lineage(keyword).collect());
        reached.extend(lineage.iter().copied());
    }
    flush(&mut reached);
    counts
}
