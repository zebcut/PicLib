//! Préréglage de filtres créé, mis à jour, renommé ou supprimé,
//! annulable par Cmd+Z.

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::filter_presets::{self, FilterPreset};

/// Nature du changement d'un préréglage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetChange {
    /// Créé.
    Created,
    /// Contenu mis à jour.
    Updated,
    /// Renommé.
    Renamed,
    /// Supprimé.
    Deleted,
}

/// Le préréglage avant et après : l'annulation remet l'état d'avant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetEdit {
    change: PresetChange,
    before: Option<FilterPreset>,
    after: Option<FilterPreset>,
}

impl PresetEdit {
    /// Création de `preset` (son identifiant vient de
    /// [`filter_presets::next_id`]).
    #[must_use]
    pub fn created(preset: FilterPreset) -> Self {
        Self {
            change: PresetChange::Created,
            before: None,
            after: Some(preset),
        }
    }

    /// Passage de `before` à `after` (même identifiant).
    #[must_use]
    pub fn changed(change: PresetChange, before: FilterPreset, after: FilterPreset) -> Self {
        Self {
            change,
            before: Some(before),
            after: Some(after),
        }
    }

    /// Suppression de `preset`.
    #[must_use]
    pub fn deleted(preset: FilterPreset) -> Self {
        Self {
            change: PresetChange::Deleted,
            before: Some(preset),
            after: None,
        }
    }

    /// Nature du changement.
    #[must_use]
    pub fn change(&self) -> PresetChange {
        self.change
    }

    /// Vrai si le préréglage ne change pas.
    #[must_use]
    pub fn changes_nothing(&self) -> bool {
        self.before == self.after
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        write(connection, self.before.as_ref(), self.after.as_ref())
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        write(connection, self.after.as_ref(), self.before.as_ref())
    }
}

/// Remplace l'état `from` par l'état `to`.
fn write(
    connection: &Connection,
    from: Option<&FilterPreset>,
    to: Option<&FilterPreset>,
) -> Result<(), CatalogError> {
    match (from, to) {
        (_, Some(preset)) => filter_presets::put(connection, preset),
        (Some(preset), None) => filter_presets::delete(connection, preset.id),
        (None, None) => Ok(()),
    }
}
