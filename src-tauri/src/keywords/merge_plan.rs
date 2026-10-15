//! Plan d'une fusion de mots-clés : les sous-mots-clés du
//! mot-clé fusionné rejoignent la cible (ou fusionnent à leur tour avec un
//! sous-mot-clé de même nom), ses photos passent sur la cible, les modèles
//! et les jeux suivent, puis il disparaît.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use super::KeywordTree;
use super::manage::ManageError;
use crate::catalog::repositories::keyword_sets::SlotRef;
use crate::catalog::repositories::metadata_templates;
use crate::domain::{KeywordId, MediaId};
use crate::edits::KeywordTreeStep;

/// Étapes d'une fusion, préparées sur l'arbre tel qu'il est avant.
pub(super) struct MergePlan<'a> {
    pub(super) tree: &'a KeywordTree,
    /// Liens existants des deux branches.
    pub(super) present: &'a HashSet<(MediaId, KeywordId)>,
    /// Photos de chaque mot-clé des deux branches.
    pub(super) media_of: &'a HashMap<KeywordId, Vec<MediaId>>,
    /// Cases de jeux qui désignent un mot-clé de la branche fusionnée.
    pub(super) slots_of: &'a HashMap<KeywordId, Vec<SlotRef>>,
    /// Pour lire les modèles de métadonnées concernés.
    pub(super) connection: &'a Connection,
    /// Mot-clé fusionné au départ.
    pub(super) merged: KeywordId,
    pub(super) steps: Vec<KeywordTreeStep>,
}

impl MergePlan<'_> {
    pub(super) fn merge(&mut self, from: KeywordId, into: KeywordId) -> Result<(), ManageError> {
        for &child in self.tree.children(Some(from)) {
            let Some(keyword) = self.tree.get(child) else {
                continue;
            };
            match self.tree.child_named(Some(into), &keyword.name) {
                // Rangé sous `into` à côté du mot-clé fusionné lui-même
                // (fusion dans son parent d'un « Paris » qui contient un
                // « Paris ») : cas trop tordu, refusé plutôt que deviné.
                Some(target) if target == self.merged => {
                    return Err(ManageError::NameTaken(keyword.name.clone()));
                }
                Some(target) => self.merge(child, target)?,
                None => self.steps.push(KeywordTreeStep::Reparent {
                    id: child,
                    before: Some(from),
                    after: Some(into),
                }),
            }
        }
        let moved = self.media_of.get(&from).map_or(&[][..], Vec::as_slice);
        let added: Vec<_> = moved
            .iter()
            .filter(|media| !self.present.contains(&(**media, into)))
            .map(|media| (*media, into))
            .collect();
        if !added.is_empty() {
            self.steps.push(KeywordTreeStep::Link(added));
        }
        if !moved.is_empty() {
            self.steps.push(KeywordTreeStep::Unlink(
                moved.iter().map(|media| (*media, from)).collect(),
            ));
        }
        // Les modèles de métadonnées et les cases des jeux suivent le
        // mot-clé qui reste.
        let templates = metadata_templates::templates_with_keyword(self.connection, from, into)?;
        if !templates.is_empty() {
            self.steps.push(KeywordTreeStep::Templates {
                templates,
                from,
                to: into,
            });
        }
        if let Some(slots) = self.slots_of.get(&from) {
            self.steps.push(KeywordTreeStep::Slots {
                slots: slots.clone(),
                before: from,
                after: into,
            });
        }
        if let Some(keyword) = self.tree.get(from) {
            self.steps.push(KeywordTreeStep::Delete(keyword.clone()));
        }
        Ok(())
    }
}
