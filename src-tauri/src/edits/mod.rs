//! Modifications du catalogue et leur annulation (Cmd+Z), communes à toute
//! l'application(section 5).
//!
//! Chaque modification est une [`Edit`] : elle retient les anciennes valeurs
//! des médias qu'elle touche, s'applique dans une transaction et rejoint
//! l'[`History`] de la session. Annuler remet les anciennes valeurs ;
//! rétablir réapplique la modification. Une nouvelle modification efface ce
//! qui pouvait être rétabli. Un dossier déplacé rejoint le
//! même historique : l'annuler le remet à sa place sur le disque.

mod capture_time;
mod collections;
mod file_metadata;
mod filter_preset;
mod keyword_tree;
mod keywords;
mod location;
mod metadata;
mod placement;
mod sidecar_read;
mod stacks;
mod template;

use std::collections::{HashMap, VecDeque};

use crate::catalog::repositories::metadata_templates::MetadataTemplate;
use crate::catalog::repositories::removal::{self, RemovedMedia};
use crate::catalog::repositories::rotation;
use crate::catalog::repositories::triage::{self, TriageValue};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{KeywordId, MediaId, MetadataField, RotationDirection};
use crate::moving::{FolderMove, FolderMoveError, MediaMove, MediaMoveKind};

pub use capture_time::{CaptureTimeChange, CaptureTimeEdit, CaptureTimeError};
pub use collections::{CollectionChange, CollectionEdit, CollectionStep};
pub use file_metadata::{FileMetadataCounts, FileMetadataEdit};
pub use filter_preset::{PresetChange, PresetEdit};
pub use keyword_tree::{KeywordTreeChange, KeywordTreeEdit, KeywordTreeStep};
pub use keywords::KeywordEdit;
pub use location::{LocationEdit, NewLocation};
pub use metadata::MetadataEdit;
pub use placement::{FolderRelocation, MediaPlace, PlacementChange, PlacementEdit};
pub use sidecar_read::{SidecarRead, SidecarReadEdit};
pub use stacks::{StackChange, StackEdit};
pub use template::TemplateEdit;

/// Nombre de modifications que l'on peut annuler.
pub const HISTORY_LIMIT: usize = 100;

/// Modification du catalogue, réversible.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Nouvelle valeur d'un attribut de tri pour plusieurs médias.
    Triage(TriageEdit),
    /// Quart de tour de plusieurs médias : son inverse est le quart de tour
    /// dans l'autre sens, sans avoir à retenir les anciennes valeurs.
    Rotation {
        /// Sens du quart de tour.
        direction: RotationDirection,
        /// Médias concernés (existants au moment de la modification).
        ids: Vec<MediaId>,
    },
    /// Retrait de médias du catalogue (les fichiers ne sont pas touchés) :
    /// leurs lignes sont gardées pour les remettre en place.
    Removal(Box<RemovedMedia>),
    /// Nouvelle valeur d'un champ de métadonnées (titre, légende…).
    Metadata(MetadataEdit),
    /// Nouvelle date de prise de vue.
    CaptureTime(CaptureTimeEdit),
    /// Mots-clés ajoutés ou retirés.
    Keywords(KeywordEdit),
    /// Arbre des mots-clés modifié (renommage, fusion, suppression…).
    KeywordTree(KeywordTreeEdit),
    /// Modèle de métadonnées appliqué.
    Template(TemplateEdit),
    /// Ce qui manquait, complété d'après les fichiers (« Lire les
    /// métadonnées des fichiers »).
    FileMetadata(FileMetadataEdit),
    /// Préréglage de filtres créé, mis à jour, renommé ou supprimé.
    FilterPreset(PresetEdit),
    /// Collections modifiées (arbre ou photos).
    Collection(CollectionEdit),
    /// Piles modifiées.
    Stack(StackEdit),
    /// Fichiers relocalisés (le catalogue suit le disque).
    Relocation(PlacementEdit),
    /// Dossier relocalisé.
    FolderRelocation(FolderRelocation),
    /// Position GPS posée ou retirée à la main.
    Location(LocationEdit),
    /// Le catalogue prend les valeurs des XMP modifiés ailleurs (« Lire
    /// depuis le fichier »).
    SidecarRead(SidecarReadEdit),
}

/// Nouvelle valeur d'un attribut de tri (marqueur, note ou couleur) pour
/// plusieurs médias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageEdit {
    after: TriageValue,
    /// Anciennes valeurs, regroupées : quelques valeurs possibles seulement,
    /// ce qui garde l'historique léger même pour 50 000 médias.
    before: Vec<(TriageValue, Vec<MediaId>)>,
}

/// Nature d'une modification, pour l'afficher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditChange {
    /// Nouvelle valeur d'un attribut de tri.
    Triage(TriageValue),
    /// Quart de tour.
    Rotation(RotationDirection),
    /// Retrait du catalogue.
    Removal,
    /// Champ de métadonnées modifié.
    Metadata(MetadataField),
    /// Date de prise de vue modifiée.
    CaptureTime,
    /// Mots-clés ajoutés.
    KeywordsAdded,
    /// Mot-clé retiré.
    KeywordRemoved,
    /// Arbre des mots-clés modifié.
    KeywordTree(KeywordTreeChange),
    /// Modèle de métadonnées appliqué.
    TemplateApplied,
    /// Métadonnées lues dans les fichiers.
    FileMetadataRead,
    /// Dossier déplacé.
    FolderMoved,
    /// Dossier renommé.
    FolderRenamed,
    /// Préréglage de filtres changé.
    FilterPreset(PresetChange),
    /// Collections modifiées.
    Collection(CollectionChange),
    /// Piles modifiées.
    Stack(StackChange),
    /// Fichier relocalisé.
    FileRelocated,
    /// Dossier relocalisé.
    FolderRelocated,
    /// Photos déplacées dans un autre dossier.
    MediaMoved,
    /// Photos renommées.
    MediaRenamed,
    /// Position posée (carte).
    LocationSet,
    /// Position retirée.
    LocationRemoved,
}

/// Ce qu'une modification a fait, pour l'afficher (« Note 3 étoiles, 12
/// photos »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSummary {
    /// Nature de la modification.
    pub change: EditChange,
    /// Nombre de médias concernés.
    pub count: usize,
}

impl Edit {
    /// Prépare le changement d'un attribut de tri : lit les valeurs actuelles
    /// des médias `ids` (les identifiants inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn triage(
        catalog: &Catalog,
        change: TriageValue,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let current = triage::read(catalog.connection(), change.attribute(), ids)?;
        let mut groups: HashMap<TriageValue, Vec<MediaId>> = HashMap::new();
        for (id, value) in current {
            groups.entry(value).or_default().push(id);
        }
        Ok(Self::Triage(TriageEdit {
            after: change,
            before: groups.into_iter().collect(),
        }))
    }

    /// Prépare un quart de tour des médias `ids` (les identifiants inconnus
    /// sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn rotation(
        catalog: &Catalog,
        direction: RotationDirection,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        let existing = rotation::read(catalog.connection(), ids)?;
        Ok(Self::Rotation {
            direction,
            ids: existing.into_iter().map(|(id, _)| id).collect(),
        })
    }

    /// Prépare le retrait des médias `ids` du catalogue (les identifiants
    /// inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn removal(catalog: &Catalog, ids: &[MediaId]) -> Result<Self, CatalogError> {
        Ok(Self::Removal(Box::new(removal::snapshot(
            catalog.connection(),
            ids,
        )?)))
    }

    /// Prépare la modification d'un champ de métadonnées des médias `ids`
    /// (`None` : vider le champ ; les identifiants inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn metadata(
        catalog: &Catalog,
        field: MetadataField,
        value: Option<String>,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        Ok(Self::Metadata(
            MetadataEdit::new(catalog.connection(), field, value, ids)?
                .with_place_keywords(catalog.connection())?,
        ))
    }

    /// Prépare le changement de date de prise de vue des médias `ids` (les
    /// identifiants inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Voir [`CaptureTimeError`].
    pub fn capture_time(
        catalog: &Catalog,
        change: CaptureTimeChange,
        ids: &[MediaId],
    ) -> Result<Self, CaptureTimeError> {
        Ok(Self::CaptureTime(CaptureTimeEdit::new(
            catalog.connection(),
            change,
            ids,
        )?))
    }

    /// Prépare la lecture des XMP `reads` : leurs valeurs remplacent celles
    /// du catalogue(« Lire depuis le fichier »).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible, ou ne peut pas être écrite (mots-clés
    /// des fichiers qui manquent).
    pub fn sidecar_read(catalog: &Catalog, reads: &[SidecarRead]) -> Result<Self, CatalogError> {
        Ok(Self::SidecarRead(SidecarReadEdit::new(
            catalog.connection(),
            reads,
        )?))
    }

    /// Prépare la position `location` des médias `ids`, avec son lieu
    /// (`None` : retirer la position ; les identifiants inconnus sont
    /// ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible, ou ne peut pas être écrite (mots-clés
    /// du nouveau lieu).
    pub fn location(
        catalog: &Catalog,
        location: Option<NewLocation>,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        Ok(Self::Location(LocationEdit::new(
            catalog.connection(),
            location,
            ids,
        )?))
    }

    /// Prépare l'ajout des mots-clés `added` aux médias `ids` (les
    /// identifiants inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn add_keywords(
        catalog: &Catalog,
        added: &[KeywordId],
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        Ok(Self::Keywords(KeywordEdit::add(
            catalog.connection(),
            added,
            ids,
        )?))
    }

    /// Prépare le retrait du mot-clé `removed` des médias `ids`.
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn remove_keyword(
        catalog: &Catalog,
        removed: KeywordId,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        Ok(Self::Keywords(KeywordEdit::remove(
            catalog.connection(),
            removed,
            ids,
        )?))
    }

    /// Prépare l'application d'un modèle de métadonnées aux médias `ids`
    /// (les identifiants inconnus sont ignorés).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn template(
        catalog: &Catalog,
        template: &MetadataTemplate,
        ids: &[MediaId],
    ) -> Result<Self, CatalogError> {
        Ok(Self::Template(TemplateEdit::new(
            catalog.connection(),
            template,
            ids,
        )?))
    }

    /// Médias concernés par la modification.
    #[must_use]
    pub fn media_ids(&self) -> Vec<MediaId> {
        match self {
            Self::Triage(edit) => edit
                .before
                .iter()
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect(),
            Self::Rotation { ids, .. } => ids.clone(),
            Self::Removal(removed) => removed.ids().to_vec(),
            Self::Metadata(edit) => edit.media_ids(),
            Self::CaptureTime(edit) => edit.media_ids(),
            Self::Keywords(edit) => edit.media_ids(),
            Self::KeywordTree(edit) => edit.media_ids(),
            Self::Template(edit) => edit.media_ids(),
            Self::FileMetadata(edit) => edit.media_ids(),
            Self::SidecarRead(edit) => edit.media_ids(),
            Self::Relocation(edit) => edit.media_ids(),
            Self::FolderRelocation(edit) => edit.media.clone(),
            Self::Location(edit) => edit.media_ids(),
            Self::FilterPreset(_) | Self::Collection(_) | Self::Stack(_) => Vec::new(),
        }
    }

    /// Résumé de la modification.
    #[must_use]
    pub fn summary(&self) -> EditSummary {
        match self {
            Self::Triage(edit) => EditSummary {
                change: EditChange::Triage(edit.after),
                count: edit.before.iter().map(|(_, ids)| ids.len()).sum(),
            },
            Self::Rotation { direction, ids } => EditSummary {
                change: EditChange::Rotation(*direction),
                count: ids.len(),
            },
            Self::Removal(removed) => EditSummary {
                change: EditChange::Removal,
                count: removed.ids().len(),
            },
            Self::Metadata(edit) => EditSummary {
                change: EditChange::Metadata(edit.field()),
                count: edit.media_ids().len(),
            },
            Self::CaptureTime(edit) => EditSummary {
                change: EditChange::CaptureTime,
                count: edit.media_ids().len(),
            },
            Self::Keywords(edit) => EditSummary {
                change: if edit.is_adding() {
                    EditChange::KeywordsAdded
                } else {
                    EditChange::KeywordRemoved
                },
                count: edit.media_ids().len(),
            },
            Self::KeywordTree(edit) => EditSummary {
                change: EditChange::KeywordTree(edit.change()),
                count: edit.media_ids().len(),
            },
            Self::Template(edit) => EditSummary {
                change: EditChange::TemplateApplied,
                count: edit.media_ids().len(),
            },
            Self::FileMetadata(edit) => EditSummary {
                change: EditChange::FileMetadataRead,
                count: edit.media_ids().len(),
            },
            Self::SidecarRead(edit) => EditSummary {
                change: EditChange::FileMetadataRead,
                count: edit.media_ids().len(),
            },
            Self::FilterPreset(edit) => EditSummary {
                change: EditChange::FilterPreset(edit.change()),
                count: 0,
            },
            Self::Collection(edit) => EditSummary {
                change: EditChange::Collection(edit.change()),
                count: edit.media_count(),
            },
            Self::Stack(edit) => EditSummary {
                change: EditChange::Stack(edit.change()),
                count: edit.media_count(),
            },
            Self::Relocation(edit) => EditSummary {
                change: EditChange::FileRelocated,
                count: edit.media_ids().len(),
            },
            Self::FolderRelocation(edit) => EditSummary {
                change: EditChange::FolderRelocated,
                count: edit.media.len(),
            },
            Self::Location(edit) => EditSummary {
                change: if edit.is_removal() {
                    EditChange::LocationRemoved
                } else {
                    EditChange::LocationSet
                },
                count: edit.media_ids().len(),
            },
        }
    }

    /// Vrai si la modification ne change rien (toutes les valeurs sont déjà
    /// celles demandées, ou aucun média).
    #[must_use]
    pub fn changes_nothing(&self) -> bool {
        match self {
            Self::Triage(edit) => edit.before.iter().all(|(value, _)| *value == edit.after),
            Self::Rotation { ids, .. } => ids.is_empty(),
            Self::Removal(removed) => removed.ids().is_empty(),
            Self::Metadata(edit) => edit.changes_nothing(),
            Self::CaptureTime(edit) => edit.changes_nothing(),
            Self::Keywords(edit) => edit.changes_nothing(),
            Self::KeywordTree(edit) => edit.changes_nothing(),
            Self::Template(edit) => edit.changes_nothing(),
            Self::FileMetadata(edit) => edit.changes_nothing(),
            Self::SidecarRead(edit) => edit.changes_nothing(),
            Self::FilterPreset(edit) => edit.changes_nothing(),
            Self::Collection(edit) => edit.changes_nothing(),
            Self::Stack(edit) => edit.changes_nothing(),
            Self::Relocation(edit) => edit.changes_nothing(),
            Self::FolderRelocation(edit) => edit.from == edit.to,
            Self::Location(edit) => edit.changes_nothing(),
        }
    }

    fn apply(&self, catalog: &mut Catalog) -> Result<(), CatalogError> {
        let transaction = catalog.transaction()?;
        match self {
            Self::Triage(edit) => {
                let ids: Vec<MediaId> = edit
                    .before
                    .iter()
                    .flat_map(|(_, ids)| ids.iter().copied())
                    .collect();
                triage::write(&transaction, edit.after, &ids)?;
            }
            Self::Rotation { direction, ids } => {
                rotation::rotate(&transaction, ids, *direction)?;
            }
            Self::Removal(removed) => {
                removal::delete(&transaction, removed.ids())?;
            }
            Self::Metadata(edit) => edit.apply(&transaction)?,
            Self::CaptureTime(edit) => edit.apply(&transaction)?,
            Self::Keywords(edit) => edit.apply(&transaction)?,
            Self::KeywordTree(edit) => edit.apply(&transaction)?,
            Self::Template(edit) => edit.apply(&transaction)?,
            Self::FileMetadata(edit) => edit.apply(&transaction)?,
            Self::SidecarRead(edit) => edit.apply(&transaction)?,
            Self::FilterPreset(edit) => edit.apply(&transaction)?,
            Self::Collection(edit) => edit.apply(&transaction)?,
            Self::Stack(edit) => edit.apply(&transaction)?,
            Self::Relocation(edit) => edit.apply(&transaction)?,
            Self::FolderRelocation(edit) => edit.apply(&transaction)?,
            Self::Location(edit) => edit.apply(&transaction)?,
        }
        transaction.commit()?;
        Ok(())
    }

    fn revert(&self, catalog: &mut Catalog) -> Result<(), CatalogError> {
        let transaction = catalog.transaction()?;
        match self {
            Self::Triage(edit) => {
                for (value, ids) in &edit.before {
                    triage::write(&transaction, *value, ids)?;
                }
            }
            Self::Rotation { direction, ids } => {
                rotation::rotate(&transaction, ids, direction.opposite())?;
            }
            Self::Removal(removed) => removal::restore(&transaction, removed)?,
            Self::Metadata(edit) => edit.revert(&transaction)?,
            Self::CaptureTime(edit) => edit.revert(&transaction)?,
            Self::Keywords(edit) => edit.revert(&transaction)?,
            Self::KeywordTree(edit) => edit.revert(&transaction)?,
            Self::Template(edit) => edit.revert(&transaction)?,
            Self::FileMetadata(edit) => edit.revert(&transaction)?,
            Self::SidecarRead(edit) => edit.revert(&transaction)?,
            Self::FilterPreset(edit) => edit.revert(&transaction)?,
            Self::Collection(edit) => edit.revert(&transaction)?,
            Self::Stack(edit) => edit.revert(&transaction)?,
            Self::Relocation(edit) => edit.revert(&transaction)?,
            Self::FolderRelocation(edit) => edit.revert(&transaction)?,
            Self::Location(edit) => edit.revert(&transaction)?,
        }
        transaction.commit()?;
        Ok(())
    }
}

/// Échec d'une annulation ou d'un rétablissement.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Le dossier n'a pas pu être remis à sa place (ou déplacé de nouveau).
    #[error(transparent)]
    FolderMove(#[from] FolderMoveError),
}

/// Entrée de l'historique : une modification du catalogue, ou un dossier
/// déplacé sur le disque et au catalogue.
#[derive(Debug, Clone, PartialEq)]
enum Entry {
    Edit(Edit),
    FolderMove(FolderMove),
    MediaMove(MediaMove),
}

impl Entry {
    fn summary(&self) -> EditSummary {
        match self {
            Self::Edit(edit) => edit.summary(),
            Self::FolderMove(moved) => EditSummary {
                change: if moved.is_rename() {
                    EditChange::FolderRenamed
                } else {
                    EditChange::FolderMoved
                },
                count: moved.media_count(),
            },
            Self::MediaMove(moved) => EditSummary {
                change: match moved.kind() {
                    MediaMoveKind::Moved => EditChange::MediaMoved,
                    MediaMoveKind::Renamed => EditChange::MediaRenamed,
                },
                count: moved.media_count(),
            },
        }
    }

    fn apply(&self, catalog: &mut Catalog) -> Result<(), EditError> {
        match self {
            Self::Edit(edit) => edit.apply(catalog)?,
            Self::FolderMove(moved) => moved.redo(catalog)?,
            Self::MediaMove(moved) => moved.redo(catalog)?,
        }
        Ok(())
    }

    fn revert(&self, catalog: &mut Catalog) -> Result<(), EditError> {
        match self {
            Self::Edit(edit) => edit.revert(catalog)?,
            Self::FolderMove(moved) => moved.undo(catalog)?,
            Self::MediaMove(moved) => moved.undo(catalog)?,
        }
        Ok(())
    }
}

/// Historique des modifications de la session : les 100 dernières peuvent
/// être annulées, et les annulations rétablies.
#[derive(Debug, Default)]
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
}

impl History {
    /// Applique une modification et l'ajoute à l'historique. Une
    /// modification qui ne change rien n'y est pas ajoutée.
    ///
    /// # Errors
    ///
    /// Échoue si la base ne peut pas être écrite ; rien n'est alors modifié.
    pub fn perform(
        &mut self,
        catalog: &mut Catalog,
        edit: Edit,
    ) -> Result<EditSummary, CatalogError> {
        let summary = edit.summary();
        if edit.changes_nothing() {
            return Ok(summary);
        }
        edit.apply(catalog)?;
        self.push(Entry::Edit(edit));
        Ok(summary)
    }

    /// Ajoute à l'historique un dossier déjà déplacé (voir
    /// [`crate::moving::move_folder`]).
    pub fn record_folder_move(&mut self, moved: FolderMove) -> EditSummary {
        let entry = Entry::FolderMove(moved);
        let summary = entry.summary();
        self.push(entry);
        summary
    }

    /// Ajoute à l'historique des photos déjà déplacées ou renommées (voir
    /// [`crate::moving::execute_media_move`]).
    pub fn record_media_move(&mut self, moved: MediaMove) -> EditSummary {
        let entry = Entry::MediaMove(moved);
        let summary = entry.summary();
        self.push(entry);
        summary
    }

    fn push(&mut self, entry: Entry) {
        self.redo.clear();
        self.undo.push_back(entry);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
    }

    /// Annule la dernière modification ; `None` s'il n'y en a pas.
    ///
    /// # Errors
    ///
    /// Voir [`EditError`] ; l'historique ne change alors pas.
    pub fn undo(&mut self, catalog: &mut Catalog) -> Result<Option<EditSummary>, EditError> {
        let Some(edit) = self.undo.pop_back() else {
            return Ok(None);
        };
        if let Err(error) = edit.revert(catalog) {
            self.undo.push_back(edit);
            return Err(error);
        }
        let summary = edit.summary();
        self.redo.push(edit);
        Ok(Some(summary))
    }

    /// Rétablit la dernière modification annulée ; `None` s'il n'y en a pas.
    ///
    /// # Errors
    ///
    /// Voir [`EditError`] ; l'historique ne change alors pas.
    pub fn redo(&mut self, catalog: &mut Catalog) -> Result<Option<EditSummary>, EditError> {
        let Some(edit) = self.redo.pop() else {
            return Ok(None);
        };
        if let Err(error) = edit.apply(catalog) {
            self.redo.push(edit);
            return Err(error);
        }
        let summary = edit.summary();
        self.undo.push_back(edit);
        Ok(Some(summary))
    }

    /// Modification que Cmd+Z annulerait.
    #[must_use]
    pub fn next_undo(&self) -> Option<EditSummary> {
        self.undo.back().map(Entry::summary)
    }

    /// Modification que Cmd+Maj+Z rétablirait.
    #[must_use]
    pub fn next_redo(&self) -> Option<EditSummary> {
        self.redo.last().map(Entry::summary)
    }
}
