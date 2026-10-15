//! Ajout et retrait annulables de mots-clés sur des médias.

use std::collections::HashSet;

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::{keyword_usage, keywords, metadata};
use crate::domain::{KeywordId, MediaId, MetadataField, Timestamp};
use crate::keywords::places;

/// Liens (média, mot-clé) ajoutés ou retirés. Seuls ceux qui changent
/// réellement sont retenus : l'annulation ne touche à rien d'autre (un
/// mot-clé déjà présent avant l'ajout reste en place).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordEdit {
    adding: bool,
    links: Vec<(MediaId, KeywordId)>,
    /// Champs de lieu vides remplis par un mot-clé de « Lieux » posé, par champ et valeur.
    fills: Vec<(MetadataField, String, Vec<MediaId>)>,
}

impl KeywordEdit {
    /// Prépare l'ajout des mots-clés `added` aux médias `ids` (les
    /// identifiants inconnus sont ignorés).
    pub(super) fn add(
        connection: &Connection,
        added: &[KeywordId],
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let media = keywords::existing_media(connection, ids)?;
        let present: HashSet<(MediaId, KeywordId)> =
            keywords::links(connection, &media)?.into_iter().collect();
        let links: Vec<(MediaId, KeywordId)> = media
            .iter()
            .flat_map(|&media| added.iter().map(move |&keyword| (media, keyword)))
            .filter(|link| !present.contains(link))
            .collect();
        let fills = places::fields_to_fill(connection, &links)?;
        Ok(Self {
            adding: true,
            links,
            fills,
        })
    }

    /// Prépare le retrait du mot-clé `removed` des médias `ids`.
    pub(super) fn remove(
        connection: &Connection,
        removed: KeywordId,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let links = keywords::links(connection, ids)?
            .into_iter()
            .filter(|&(_, keyword)| keyword == removed)
            .collect();
        Ok(Self {
            adding: false,
            links,
            fills: Vec::new(),
        })
    }

    /// Vrai pour un ajout, faux pour un retrait.
    #[must_use]
    pub fn is_adding(&self) -> bool {
        self.adding
    }

    /// Médias concernés, sans doublon.
    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        let mut seen = HashSet::new();
        self.links
            .iter()
            .map(|&(media, _)| media)
            .filter(|media| seen.insert(*media))
            .collect()
    }

    pub(super) fn changes_nothing(&self) -> bool {
        self.links.is_empty()
    }

    /// Même ajout, sans remplir les champs `fields` (un modèle qui les
    /// donne lui-même l'emporte).
    pub(super) fn without_fills_of(mut self, fields: &[MetadataField]) -> Self {
        self.fills.retain(|(field, _, _)| !fields.contains(field));
        self
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        if self.adding {
            keywords::link(connection, &self.links)?;
            for (field, value, ids) in &self.fills {
                metadata::write(connection, *field, Some(value), ids)?;
            }
            // Les mots-clés posés deviennent les plus récents (suggestions) ; annuler ne les rend pas moins récents.
            let mut seen = HashSet::new();
            let used: Vec<KeywordId> = self
                .links
                .iter()
                .map(|&(_, keyword)| keyword)
                .filter(|keyword| seen.insert(*keyword))
                .collect();
            keyword_usage::mark_used(connection, &used, Timestamp::now())
        } else {
            keywords::unlink(connection, &self.links)
        }
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        if self.adding {
            for (field, _, ids) in &self.fills {
                metadata::write(connection, *field, None, ids)?;
            }
            keywords::unlink(connection, &self.links)
        } else {
            keywords::link(connection, &self.links)
        }
    }
}
