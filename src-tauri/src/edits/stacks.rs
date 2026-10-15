//! Modifications annulables des piles : empiler, dépiler,
//! fusionner, changer la couverture, retirer une photo, empiler les paires
//! RAW + JPEG ou les rafales.
//!
//! Chaque modification remplace des piles par d'autres : `before` (telles
//! qu'elles étaient) par `after`. Annuler fait l'inverse.

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::stacks::{self, Stack};

/// Nature d'une modification des piles, pour l'afficher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackChange {
    /// Photos empilées (ou piles fusionnées).
    Stacked,
    /// Piles défaites.
    Unstacked,
    /// Couverture changée.
    CoverChanged,
    /// Photos retirées de leur pile.
    Removed,
    /// Paires RAW + JPEG empilées.
    PairsStacked,
    /// Rafales empilées.
    BurstsStacked,
}

/// Piles remplacées par d'autres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackEdit {
    change: StackChange,
    before: Vec<Stack>,
    after: Vec<Stack>,
}

impl StackEdit {
    /// Remplace les piles `before` (telles qu'elles sont) par `after`.
    #[must_use]
    pub fn new(change: StackChange, before: Vec<Stack>, after: Vec<Stack>) -> Self {
        Self {
            change,
            before,
            after,
        }
    }

    /// Nature de la modification.
    #[must_use]
    pub fn change(&self) -> StackChange {
        self.change
    }

    /// Photos des piles touchées, chacune une fois.
    #[must_use]
    pub fn media_count(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        self.before
            .iter()
            .chain(&self.after)
            .flat_map(|stack| &stack.members)
            .filter(|id| seen.insert(**id))
            .count()
    }

    /// Vrai si rien ne change.
    #[must_use]
    pub fn changes_nothing(&self) -> bool {
        self.before == self.after
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        replace(connection, &self.before, &self.after)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        replace(connection, &self.after, &self.before)
    }
}

fn replace(connection: &Connection, old: &[Stack], new: &[Stack]) -> Result<(), CatalogError> {
    for stack in old {
        stacks::delete(connection, stack.id)?;
    }
    for stack in new {
        stacks::insert(connection, stack)?;
    }
    Ok(())
}
