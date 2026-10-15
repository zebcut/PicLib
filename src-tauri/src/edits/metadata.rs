//! Modification annulable d'un champ de métadonnées descriptives.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::metadata;
use crate::domain::{MediaId, MetadataField};
use crate::keywords::places::{self, PLACE_FIELDS, PlaceLinks};

/// Nouvelle valeur d'un champ de métadonnées pour plusieurs médias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEdit {
    field: MetadataField,
    after: Option<String>,
    /// Anciennes valeurs, regroupées : souvent quelques valeurs seulement
    /// (tous vides, le même auteur…), ce qui garde l'historique léger.
    before: Vec<(Option<String>, Vec<MediaId>)>,
    /// Champ de lieu : mots-clés de lieu à remplacer.
    places: PlaceLinks,
}

impl MetadataEdit {
    /// Prépare la modification : lit les valeurs actuelles des médias `ids`
    /// (les identifiants inconnus sont ignorés).
    pub(super) fn new(
        connection: &Connection,
        field: MetadataField,
        after: Option<String>,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let mut groups: HashMap<Option<String>, Vec<MediaId>> = HashMap::new();
        for (id, value) in metadata::read(connection, field, ids)? {
            groups.entry(value).or_default().push(id);
        }
        Ok(Self {
            field,
            after,
            before: groups.into_iter().collect(),
            places: PlaceLinks::default(),
        })
    }

    /// Même modification, qui remplace aussi le mot-clé de lieu des médias
    /// si le champ est un champ de lieu(les champs font foi).
    pub(super) fn with_place_keywords(
        mut self,
        connection: &Connection,
    ) -> Result<Self, CatalogError> {
        if !PLACE_FIELDS.contains(&self.field) {
            return Ok(self);
        }
        let before = places::read(connection, &self.media_ids())?;
        let after = before
            .iter()
            .map(|(id, fields)| (*id, fields.with(self.field, self.after.as_deref())))
            .collect();
        self.places = places::changes(connection, &before, &after)?;
        Ok(self)
    }

    /// Champ modifié.
    #[must_use]
    pub fn field(&self) -> MetadataField {
        self.field
    }

    /// Médias concernés.
    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        self.before
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    /// Vrai si tous les médias ont déjà la nouvelle valeur.
    pub(super) fn changes_nothing(&self) -> bool {
        self.before.iter().all(|(value, _)| *value == self.after)
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        metadata::write(
            connection,
            self.field,
            self.after.as_deref(),
            &self.media_ids(),
        )?;
        self.places.apply(connection)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.places.revert(connection)?;
        for (value, ids) in &self.before {
            metadata::write(connection, self.field, value.as_deref(), ids)?;
        }
        Ok(())
    }
}
