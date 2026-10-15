//! Mots-clés hiérarchiques : arbre, chemins saisis,
//! autocomplétion, mots-clés d'une sélection, gestion de la liste, jeux,
//! suggestions, import et export de la liste, mots-clés lus dans les
//! fichiers.

mod counts;
mod entry;
pub mod manage;
mod merge_plan;
pub mod places;
mod recovery;
mod selection;
pub mod sets;
mod suggested;
pub mod text_format;
pub mod transfer;
mod tree;

pub use counts::branch_photo_counts;
pub use entry::{KeywordError, Suggestion, resolve_entry, suggestions};
pub use recovery::resolve_recovered;
pub use selection::{Presence, SelectionKeyword, SelectionKeywords, selection_keywords};
pub use suggested::{MAX_SUGGESTED_KEYWORDS, suggested_keywords};
pub use tree::KeywordTree;

/// Nombre maximal de suggestions de l'autocomplétion.
pub const MAX_SUGGESTIONS: usize = 10;
