//! Modifications annulables des collections(10.3 et 10.4) :
//! créer, renommer, déplacer, supprimer, ajouter ou retirer des photos, et
//! la collection rapide.
//!
//! Chaque modification est une suite d'étapes élémentaires dont on connaît
//! l'inverse : annuler rejoue les inverses dans l'ordre contraire. Les
//! étapes sont préparées par `collections` (le service), qui lit l'arbre.

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::collections::{self, CollectionNode};
use crate::catalog::repositories::quick_collection;
use crate::domain::{CollectionId, MediaId};

/// Nature d'une modification des collections, pour l'afficher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionChange {
    /// Collection ou dossier créé.
    Created,
    /// Renommé.
    Renamed,
    /// Déplacé dans un autre dossier.
    Moved,
    /// Supprimé (avec son contenu, pour un dossier).
    Deleted,
    /// Photos ajoutées à une collection.
    MediaAdded,
    /// Photos retirées d'une collection.
    MediaRemoved,
    /// Photos ajoutées à la collection rapide.
    QuickAdded,
    /// Photos retirées de la collection rapide.
    QuickRemoved,
    /// Collection rapide vidée.
    QuickCleared,
    /// Collection rapide enregistrée comme collection.
    QuickSaved,
    /// Règles d'une collection dynamique modifiées.
    RulesChanged,
}

/// Étape élémentaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionStep {
    /// Crée ce nœud (son identifiant compris) ; l'inverse le supprime.
    Create(CollectionNode),
    /// Supprime ce nœud (vide à ce moment-là) ; l'inverse le remet.
    Delete(CollectionNode),
    /// Change le nom.
    Rename {
        /// Nœud.
        id: CollectionId,
        /// Nom avant.
        before: String,
        /// Nom après.
        after: String,
    },
    /// Range sous un autre dossier.
    Reparent {
        /// Nœud.
        id: CollectionId,
        /// Dossier avant.
        before: Option<CollectionId>,
        /// Dossier après.
        after: Option<CollectionId>,
    },
    /// Met ces photos (absentes avant) dans la collection.
    Add {
        /// Collection.
        collection: CollectionId,
        /// Photos.
        media: Vec<MediaId>,
    },
    /// Retire ces photos (présentes avant) de la collection.
    Remove {
        /// Collection.
        collection: CollectionId,
        /// Photos.
        media: Vec<MediaId>,
    },
    /// Remplace les règles d'une collection dynamique.
    SetRules {
        /// Collection.
        id: CollectionId,
        /// Règles avant.
        before: Option<String>,
        /// Règles après.
        after: Option<String>,
    },
    /// Met ces photos (absentes avant) dans la collection rapide.
    QuickAdd(Vec<MediaId>),
    /// Retire ces photos (présentes avant) de la collection rapide.
    QuickRemove(Vec<MediaId>),
}

/// Modification des collections : ses étapes, dans l'ordre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEdit {
    change: CollectionChange,
    steps: Vec<CollectionStep>,
}

impl CollectionEdit {
    /// Modification faite de ces étapes.
    #[must_use]
    pub fn new(change: CollectionChange, steps: Vec<CollectionStep>) -> Self {
        Self { change, steps }
    }

    /// Nature de la modification.
    #[must_use]
    pub fn change(&self) -> CollectionChange {
        self.change
    }

    /// Photos ajoutées ou retirées (chacune une fois par collection).
    #[must_use]
    pub fn media_count(&self) -> usize {
        self.steps
            .iter()
            .map(|step| match step {
                CollectionStep::Add { media, .. }
                | CollectionStep::Remove { media, .. }
                | CollectionStep::QuickAdd(media)
                | CollectionStep::QuickRemove(media) => media.len(),
                _ => 0,
            })
            .sum()
    }

    /// Vrai si rien ne change.
    #[must_use]
    pub fn changes_nothing(&self) -> bool {
        self.steps.iter().all(|step| match step {
            CollectionStep::Add { media, .. }
            | CollectionStep::Remove { media, .. }
            | CollectionStep::QuickAdd(media)
            | CollectionStep::QuickRemove(media) => media.is_empty(),
            CollectionStep::Rename { before, after, .. } => before == after,
            CollectionStep::Reparent { before, after, .. } => before == after,
            CollectionStep::SetRules { before, after, .. } => before == after,
            CollectionStep::Create(_) | CollectionStep::Delete(_) => false,
        })
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for step in &self.steps {
            forward(connection, step)?;
        }
        Ok(())
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        for step in self.steps.iter().rev() {
            backward(connection, step)?;
        }
        Ok(())
    }
}

fn forward(connection: &Connection, step: &CollectionStep) -> Result<(), CatalogError> {
    match step {
        CollectionStep::Create(node) => collections::insert(connection, node),
        CollectionStep::Delete(node) => collections::delete(connection, node.id),
        CollectionStep::Rename { id, after, .. } => collections::rename(connection, *id, after),
        CollectionStep::Reparent { id, after, .. } => {
            collections::reparent(connection, *id, *after)
        }
        CollectionStep::Add { collection, media } => {
            collections::add(connection, *collection, media)
        }
        CollectionStep::Remove { collection, media } => {
            collections::remove(connection, *collection, media)
        }
        CollectionStep::QuickAdd(media) => quick_collection::add(connection, media),
        CollectionStep::QuickRemove(media) => quick_collection::remove(connection, media),
        CollectionStep::SetRules { id, after, .. } => {
            collections::set_rules(connection, *id, after.as_deref())
        }
    }
}

fn backward(connection: &Connection, step: &CollectionStep) -> Result<(), CatalogError> {
    match step {
        CollectionStep::Create(node) => collections::delete(connection, node.id),
        CollectionStep::Delete(node) => collections::insert(connection, node),
        CollectionStep::Rename { id, before, .. } => collections::rename(connection, *id, before),
        CollectionStep::Reparent { id, before, .. } => {
            collections::reparent(connection, *id, *before)
        }
        CollectionStep::Add { collection, media } => {
            collections::remove(connection, *collection, media)
        }
        CollectionStep::Remove { collection, media } => {
            collections::add(connection, *collection, media)
        }
        CollectionStep::QuickAdd(media) => quick_collection::remove(connection, media),
        CollectionStep::QuickRemove(media) => quick_collection::add(connection, media),
        CollectionStep::SetRules { id, before, .. } => {
            collections::set_rules(connection, *id, before.as_deref())
        }
    }
}
