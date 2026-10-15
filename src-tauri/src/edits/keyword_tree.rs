//! Modifications annulables de l'arbre des mots-clés :
//! renommer, déplacer, fusionner, supprimer, « inclure à l'export »,
//! importer une liste.
//!
//! Chaque modification est une suite d'étapes élémentaires dont on connaît
//! l'inverse : annuler rejoue les inverses dans l'ordre contraire. Les
//! étapes sont préparées par `keywords::manage`, qui lit l'arbre.

use crate::catalog::CatalogError;
use crate::catalog::repositories::keyword_changes;
use crate::catalog::repositories::keyword_sets::{self, SlotRef};
use crate::catalog::repositories::keywords::{self, Keyword};
use crate::catalog::repositories::metadata_templates;
use crate::catalog::repositories::xmp_sync;
use crate::domain::{KeywordId, MediaId, MetadataTemplateId};

/// Nature d'une modification de l'arbre, pour l'afficher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordTreeChange {
    /// Mot-clé renommé.
    Renamed,
    /// Mot-clé déplacé dans l'arbre, avec sa branche.
    Moved,
    /// Mot-clé fusionné avec un autre.
    Merged,
    /// Mots-clés supprimés.
    Deleted,
    /// « Inclure à l'export » coché (`true`) ou décoché.
    Export(bool),
    /// Liste de mots-clés importée (nombre de mots-clés créés).
    Imported(usize),
}

/// Étape élémentaire d'une modification de l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordTreeStep {
    /// Supprime ce mot-clé (sans enfant ni lien à ce moment-là) ; l'inverse
    /// le remet avec son identifiant.
    Delete(Keyword),
    /// Crée ce mot-clé avec son identifiant (import d'une liste) ;
    /// l'inverse le supprime.
    Create(Keyword),
    /// Change le nom d'un mot-clé.
    Rename {
        /// Mot-clé.
        id: KeywordId,
        /// Nom avant.
        before: String,
        /// Nom après.
        after: String,
    },
    /// Range un mot-clé sous un autre parent.
    Reparent {
        /// Mot-clé.
        id: KeywordId,
        /// Parent avant.
        before: Option<KeywordId>,
        /// Parent après.
        after: Option<KeywordId>,
    },
    /// Coche ou décoche « inclure à l'export ».
    Export {
        /// Mot-clé.
        id: KeywordId,
        /// Valeur après (l'inverse remet l'autre).
        include: bool,
    },
    /// Pose ces mots-clés sur ces photos (liens absents avant).
    Link(Vec<(MediaId, KeywordId)>),
    /// Retire ces mots-clés de ces photos (liens présents avant).
    Unlink(Vec<(MediaId, KeywordId)>),
    /// Fait passer les modèles de métadonnées de `from` à `to` (fusion) ;
    /// un modèle qui contenait déjà `to` (`true`) perd seulement `from`.
    Templates {
        /// Modèles concernés.
        templates: Vec<(MetadataTemplateId, bool)>,
        /// Mot-clé fusionné.
        from: KeywordId,
        /// Mot-clé qui reste.
        to: KeywordId,
    },
    /// Fait désigner un autre mot-clé à des cases de jeux (fusion).
    Slots {
        /// Cases concernées.
        slots: Vec<SlotRef>,
        /// Mot-clé désigné avant.
        before: KeywordId,
        /// Mot-clé désigné après.
        after: KeywordId,
    },
}

impl KeywordTreeStep {
    fn run(&self, connection: &rusqlite::Connection, forward: bool) -> Result<(), CatalogError> {
        match self {
            Self::Delete(keyword) if forward => keyword_changes::delete(connection, keyword.id),
            Self::Delete(keyword) => keyword_changes::insert(connection, keyword),
            Self::Create(keyword) if forward => keyword_changes::insert(connection, keyword),
            Self::Create(keyword) => keyword_changes::delete(connection, keyword.id),
            Self::Rename { id, before, after } => {
                keyword_changes::rename(connection, *id, if forward { after } else { before })
            }
            Self::Reparent { id, before, after } => {
                keyword_changes::set_parent(connection, *id, if forward { *after } else { *before })
            }
            Self::Export { id, include } => {
                keyword_changes::set_include_on_export(connection, *id, *include == forward)
            }
            Self::Link(links) if forward => keywords::link(connection, links),
            Self::Link(links) => keywords::unlink(connection, links),
            Self::Unlink(links) if forward => keywords::unlink(connection, links),
            Self::Unlink(links) => keywords::link(connection, links),
            Self::Templates {
                templates,
                from,
                to,
            } => retarget_templates(connection, templates, *from, *to, forward),
            Self::Slots {
                slots,
                before,
                after,
            } => keyword_sets::retarget(connection, slots, if forward { *after } else { *before }),
        }
    }
}

/// Fait passer des modèles de `from` à `to` (`forward`), ou l'inverse.
fn retarget_templates(
    connection: &rusqlite::Connection,
    templates: &[(MetadataTemplateId, bool)],
    from: KeywordId,
    to: KeywordId,
    forward: bool,
) -> Result<(), CatalogError> {
    for &(template, had_target) in templates {
        match (had_target, forward) {
            (false, true) => metadata_templates::replace_keyword(connection, template, from, to)?,
            (false, false) => metadata_templates::replace_keyword(connection, template, to, from)?,
            (true, true) => metadata_templates::remove_keyword(connection, template, from)?,
            (true, false) => metadata_templates::add_keyword(connection, template, from)?,
        }
    }
    Ok(())
}

/// Modification de l'arbre des mots-clés, réversible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordTreeEdit {
    change: KeywordTreeChange,
    steps: Vec<KeywordTreeStep>,
    /// Photos dont les mots-clés (ou leurs chemins) changent.
    media: Vec<MediaId>,
}

impl KeywordTreeEdit {
    /// Modification faite des étapes `steps`, dans l'ordre ; `media` : les
    /// photos concernées, sans doublon.
    #[must_use]
    pub fn new(
        change: KeywordTreeChange,
        steps: Vec<KeywordTreeStep>,
        media: Vec<MediaId>,
    ) -> Self {
        Self {
            change,
            steps,
            media,
        }
    }

    /// Nature de la modification.
    #[must_use]
    pub fn change(&self) -> KeywordTreeChange {
        self.change
    }

    /// Étapes de la modification, dans l'ordre où elles s'appliquent.
    #[must_use]
    pub fn steps(&self) -> &[KeywordTreeStep] {
        &self.steps
    }

    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        self.media.clone()
    }

    pub(super) fn changes_nothing(&self) -> bool {
        self.steps.is_empty()
    }

    pub(super) fn apply(&self, connection: &rusqlite::Connection) -> Result<(), CatalogError> {
        self.steps
            .iter()
            .try_for_each(|step| step.run(connection, true))?;
        // Le chemin des mots-clés de ces photos change dans leur XMP, sans
        // toucher à leurs liens.
        xmp_sync::mark_changed(connection, &self.media)
    }

    pub(super) fn revert(&self, connection: &rusqlite::Connection) -> Result<(), CatalogError> {
        self.steps
            .iter()
            .rev()
            .try_for_each(|step| step.run(connection, false))?;
        xmp_sync::mark_changed(connection, &self.media)
    }
}
