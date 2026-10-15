//! Application annulable d'un modèle de métadonnées : ses champs
//! remplis ou vidés et ses mots-clés ajoutés, en une seule modification.

use rusqlite::Connection;

use super::keywords::KeywordEdit;
use super::metadata::MetadataEdit;
use crate::catalog::CatalogError;
use crate::catalog::repositories::keywords;
use crate::catalog::repositories::metadata_templates::{FieldAction, MetadataTemplate};
use crate::domain::MediaId;
use crate::keywords::places::{self, PLACE_FIELDS, PlaceLinks};

/// Modèle appliqué à des médias : une modification par champ, plus l'ajout
/// des mots-clés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEdit {
    fields: Vec<MetadataEdit>,
    keywords: Option<KeywordEdit>,
    /// Mots-clés de lieu remplacés si le modèle change des champs de lieu.
    places: PlaceLinks,
    /// Médias concernés (existants au moment de la modification).
    media: Vec<MediaId>,
}

impl TemplateEdit {
    /// Prépare l'application de `template` aux médias `ids` (les
    /// identifiants inconnus sont ignorés).
    pub(super) fn new(
        connection: &Connection,
        template: &MetadataTemplate,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let media = keywords::existing_media(connection, ids)?;
        let fields = template
            .fields
            .iter()
            .map(|(field, action)| {
                let value = match action {
                    FieldAction::Fill(value) => Some(value.clone()),
                    FieldAction::Clear => None,
                };
                MetadataEdit::new(connection, *field, value, &media)
            })
            .collect::<Result<_, _>>()?;
        let keywords = if template.keywords.is_empty() {
            None
        } else {
            let given: Vec<_> = template.fields.iter().map(|(field, _)| *field).collect();
            Some(KeywordEdit::add(connection, &template.keywords, &media)?.without_fills_of(&given))
        };
        let places = if template
            .fields
            .iter()
            .any(|(field, _)| PLACE_FIELDS.contains(field))
        {
            let before = places::read(connection, &media)?;
            let after = before
                .iter()
                .map(|(id, current)| {
                    let changed =
                        template
                            .fields
                            .iter()
                            .fold(current.clone(), |fields, (field, action)| {
                                fields.with(
                                    *field,
                                    match action {
                                        FieldAction::Fill(value) => Some(value),
                                        FieldAction::Clear => None,
                                    },
                                )
                            });
                    (*id, changed)
                })
                .collect();
            places::changes(connection, &before, &after)?
        } else {
            PlaceLinks::default()
        };
        Ok(Self {
            fields,
            keywords,
            places,
            media,
        })
    }

    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        self.media.clone()
    }

    pub(super) fn changes_nothing(&self) -> bool {
        self.fields.iter().all(MetadataEdit::changes_nothing)
            && self
                .keywords
                .as_ref()
                .is_none_or(KeywordEdit::changes_nothing)
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for field in &self.fields {
            field.apply(connection)?;
        }
        if let Some(keywords) = &self.keywords {
            keywords.apply(connection)?;
        }
        self.places.apply(connection)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.places.revert(connection)?;
        if let Some(keywords) = &self.keywords {
            keywords.revert(connection)?;
        }
        for field in self.fields.iter().rev() {
            field.revert(connection)?;
        }
        Ok(())
    }
}
