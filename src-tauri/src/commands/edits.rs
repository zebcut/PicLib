//! Modifications du catalogue (marqueurs, notes, couleurs, rotation) et leur
//! annulation ; les métadonnées descriptives ont leurs commandes dans
//! `metadata`, la date de prise de vue dans `capture_time`, les mots-clés
//! dans `keywords`.

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::catalog::repositories::triage::TriageValue;
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{ColorLabel, Flag, MediaId, Rating, RotationDirection};
use crate::edits::{
    CollectionChange, Edit, EditChange, EditError, EditSummary, History, KeywordTreeChange,
    PresetChange, StackChange,
};
use crate::moving::FolderMoveError;

use super::metadata::MetadataFieldDto;
use super::preferences::AppPreferences;
use crate::stacks;

/// Marqueur, tel qu'échangé avec l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FlagDto {
    /// Retenue.
    Picked,
    /// Rejetée.
    Rejected,
}

/// Étiquette de couleur, telle qu'échangée avec l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ColorLabelDto {
    /// Rouge.
    Red,
    /// Jaune.
    Yellow,
    /// Vert.
    Green,
    /// Bleu.
    Blue,
    /// Violet.
    Purple,
}

/// Nouvelle valeur d'un attribut de tri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TriageChange {
    /// Marqueur (`null` : aucun).
    Flag {
        /// Nouveau marqueur.
        flag: Option<FlagDto>,
    },
    /// Note.
    Rating {
        /// Nombre d'étoiles, de 0 à 5.
        stars: u8,
    },
    /// Étiquette de couleur (`null` : aucune).
    ColorLabel {
        /// Nouvelle couleur.
        color: Option<ColorLabelDto>,
    },
}

/// Sens d'un quart de tour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RotationDirectionDto {
    /// Vers la gauche (Cmd+[).
    Left,
    /// Vers la droite (Cmd+]).
    Right,
}

/// Nature d'une modification, pour l'afficher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EditChangeDto {
    /// Marqueur (`null` : aucun).
    Flag {
        /// Nouveau marqueur.
        flag: Option<FlagDto>,
    },
    /// Note.
    Rating {
        /// Nombre d'étoiles, de 0 à 5.
        stars: u8,
    },
    /// Étiquette de couleur (`null` : aucune).
    ColorLabel {
        /// Nouvelle couleur.
        color: Option<ColorLabelDto>,
    },
    /// Quart de tour.
    Rotation {
        /// Sens du quart de tour.
        direction: RotationDirectionDto,
    },
    /// Retrait du catalogue (fichiers intacts).
    Removal,
    /// Champ de métadonnées modifié.
    Metadata {
        /// Champ modifié.
        field: MetadataFieldDto,
    },
    /// Date de prise de vue modifiée.
    CaptureTime,
    /// Position posée sur la carte.
    LocationSet,
    /// Position retirée.
    LocationRemoved,
    /// Mots-clés ajoutés.
    KeywordsAdded,
    /// Mot-clé retiré.
    KeywordRemoved,
    /// Mot-clé renommé.
    KeywordRenamed,
    /// Mot-clé déplacé dans l'arbre.
    KeywordMoved,
    /// Mot-clé fusionné avec un autre.
    KeywordsMerged,
    /// Mots-clés supprimés.
    KeywordsDeleted,
    /// « Inclure à l'export » coché ou décoché.
    KeywordExport {
        /// Nouvelle valeur.
        include: bool,
    },
    /// Modèle de métadonnées appliqué.
    TemplateApplied,
    /// Liste de mots-clés importée.
    KeywordsImported {
        /// Nombre de mots-clés créés.
        count: u32,
    },
    /// Métadonnées lues dans les fichiers.
    FileMetadataRead,
    /// Dossier déplacé.
    FolderMoved,
    /// Préréglage de filtres créé.
    FilterPresetCreated,
    /// Préréglage de filtres mis à jour.
    FilterPresetUpdated,
    /// Préréglage de filtres renommé.
    FilterPresetRenamed,
    /// Préréglage de filtres supprimé.
    FilterPresetDeleted,
    /// Collection ou dossier de collections créé.
    CollectionCreated,
    /// Collection ou dossier renommé.
    CollectionRenamed,
    /// Collection ou dossier déplacé.
    CollectionMoved,
    /// Collection ou dossier supprimé.
    CollectionDeleted,
    /// Photos ajoutées à une collection.
    CollectionMediaAdded,
    /// Photos retirées d'une collection.
    CollectionMediaRemoved,
    /// Photos ajoutées à la collection rapide.
    QuickCollectionAdded,
    /// Photos retirées de la collection rapide.
    QuickCollectionRemoved,
    /// Collection rapide vidée.
    QuickCollectionCleared,
    /// Collection rapide enregistrée comme collection.
    QuickCollectionSaved,
    /// Règles d'une collection dynamique modifiées.
    SmartRulesChanged,
    /// Photos empilées.
    Stacked,
    /// Piles défaites.
    Unstacked,
    /// Couverture d'une pile changée.
    StackCoverChanged,
    /// Photos retirées de leur pile.
    RemovedFromStack,
    /// Paires RAW + JPEG empilées.
    PairsStacked,
    /// Rafales empilées.
    BurstsStacked,
    /// Fichier relocalisé.
    FileRelocated,
    /// Dossier relocalisé.
    FolderRelocated,
    /// Photos déplacées.
    MediaMoved,
    /// Photos renommées.
    MediaRenamed,
    /// Dossier renommé.
    FolderRenamed,
}

/// Modification faite, annulée ou rétablie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditSummaryDto {
    /// Nature de la modification.
    pub change: EditChangeDto,
    /// Nombre de médias concernés.
    pub count: u32,
}

/// Ce que Cmd+Z et Cmd+Maj+Z feraient, pour les libellés du menu Édition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryState {
    /// Modification que Cmd+Z annulerait.
    pub undo: Option<EditSummaryDto>,
    /// Modification que Cmd+Maj+Z rétablirait.
    pub redo: Option<EditSummaryDto>,
}

/// Résultat d'une modification, d'une annulation ou d'un rétablissement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    /// Ce qui a été fait ; `null` s'il n'y avait rien à annuler ou rétablir.
    pub summary: Option<EditSummaryDto>,
    /// État de l'historique après coup.
    pub history: HistoryState,
}

/// Raison pour laquelle une modification n'a pas eu lieu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EditFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Valeur hors limites (note au-delà de 5 étoiles).
    InvalidValue,
    /// Un dossier n'a pas pu être déplacé ou remis à sa place sur le disque.
    FilesUnavailable,
}

/// Donne un marqueur, une note ou une couleur aux médias indiqués, de façon
/// annulable.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
// Tâche asynchrone : modifier des dizaines de milliers de photos ne doit pas
// figer l'interface.
#[tauri::command]
#[specta::specta]
pub async fn set_triage(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    preferences: State<'_, AppPreferences>,
    media_ids: Vec<u32>,
    change: TriageChange,
) -> Result<EditOutcome, EditFailure> {
    let value = TriageValue::try_from(change)?;
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue : le même ordre partout évite
    // tout blocage mutuel.
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    // Une pile repliée : toute la pile, si la préférence le demande.
    let ids = if preferences.current().triage_whole_stack {
        stacks::with_collapsed_members(&catalog, &ids).map_err(|error| failure(&error))?
    } else {
        ids
    };
    let summary = Edit::triage(&catalog, value, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Fait pivoter les médias indiqués d'un quart de tour, de façon annulable
/// (affichage seulement : les fichiers ne sont jamais modifiés).
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn rotate(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    direction: RotationDirectionDto,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = Edit::rotation(&catalog, direction.into(), &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Annule la dernière modification.
///
/// # Errors
///
/// Voir [`EditFailure`] ; l'historique ne change pas en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn undo_edit(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<EditOutcome, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = history
        .undo(&mut catalog)
        .map_err(|error| history_failure(&error))?;
    Ok(outcome(summary, &history))
}

/// Rétablit la dernière modification annulée.
///
/// # Errors
///
/// Voir [`EditFailure`] ; l'historique ne change pas en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn redo_edit(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<EditOutcome, EditFailure> {
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = history
        .redo(&mut catalog)
        .map_err(|error| history_failure(&error))?;
    Ok(outcome(summary, &history))
}

/// Ce que Cmd+Z et Cmd+Maj+Z feraient maintenant.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_edit_history(history: State<'_, Mutex<History>>) -> HistoryState {
    history_state(&lock(&history))
}

pub(super) fn media_ids_of(ids: Vec<u32>) -> Vec<MediaId> {
    ids.into_iter()
        .map(|id| MediaId::new(i64::from(id)))
        .collect()
}

pub(super) fn outcome(summary: Option<EditSummary>, history: &History) -> EditOutcome {
    EditOutcome {
        summary: summary.map(EditSummaryDto::from),
        history: history_state(history),
    }
}

fn history_state(history: &History) -> HistoryState {
    HistoryState {
        undo: history.next_undo().map(EditSummaryDto::from),
        redo: history.next_redo().map(EditSummaryDto::from),
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Un verrou « empoisonné » garde des données cohérentes : chaque
    // modification est faite dans une transaction.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn history_failure(error: &EditError) -> EditFailure {
    match error {
        EditError::Catalog(error) | EditError::FolderMove(FolderMoveError::Catalog(error)) => {
            failure(error)
        }
        EditError::FolderMove(error @ FolderMoveError::Disk(_)) => {
            tracing::error!(%error, "cannot undo or redo a folder move");
            EditFailure::FilesUnavailable
        }
    }
}

pub(super) fn failure(error: &CatalogError) -> EditFailure {
    tracing::error!(%error, "cannot modify the catalog");
    EditFailure::CatalogUnavailable
}

impl TryFrom<TriageChange> for TriageValue {
    type Error = EditFailure;

    fn try_from(change: TriageChange) -> Result<Self, EditFailure> {
        Ok(match change {
            TriageChange::Flag { flag } => Self::Flag(flag.map(Flag::from)),
            TriageChange::Rating { stars } => {
                Self::Rating(Rating::new(stars).ok_or(EditFailure::InvalidValue)?)
            }
            TriageChange::ColorLabel { color } => Self::ColorLabel(color.map(ColorLabel::from)),
        })
    }
}

impl From<TriageValue> for TriageChange {
    fn from(value: TriageValue) -> Self {
        match value {
            TriageValue::Flag(flag) => Self::Flag {
                flag: flag.map(FlagDto::from),
            },
            TriageValue::Rating(rating) => Self::Rating {
                stars: rating.stars(),
            },
            TriageValue::ColorLabel(color) => Self::ColorLabel {
                color: color.map(ColorLabelDto::from),
            },
        }
    }
}

impl From<EditChange> for EditChangeDto {
    fn from(change: EditChange) -> Self {
        match change {
            EditChange::Triage(value) => match TriageChange::from(value) {
                TriageChange::Flag { flag } => Self::Flag { flag },
                TriageChange::Rating { stars } => Self::Rating { stars },
                TriageChange::ColorLabel { color } => Self::ColorLabel { color },
            },
            EditChange::Rotation(direction) => Self::Rotation {
                direction: direction.into(),
            },
            EditChange::Removal => Self::Removal,
            EditChange::Metadata(field) => Self::Metadata {
                field: field.into(),
            },
            EditChange::CaptureTime => Self::CaptureTime,
            EditChange::LocationSet => Self::LocationSet,
            EditChange::LocationRemoved => Self::LocationRemoved,
            EditChange::KeywordsAdded => Self::KeywordsAdded,
            EditChange::KeywordRemoved => Self::KeywordRemoved,
            EditChange::TemplateApplied => Self::TemplateApplied,
            EditChange::FileMetadataRead => Self::FileMetadataRead,
            EditChange::FolderMoved => Self::FolderMoved,
            EditChange::FolderRenamed => Self::FolderRenamed,
            EditChange::FilterPreset(change) => match change {
                PresetChange::Created => Self::FilterPresetCreated,
                PresetChange::Updated => Self::FilterPresetUpdated,
                PresetChange::Renamed => Self::FilterPresetRenamed,
                PresetChange::Deleted => Self::FilterPresetDeleted,
            },
            EditChange::Collection(change) => match change {
                CollectionChange::Created => Self::CollectionCreated,
                CollectionChange::Renamed => Self::CollectionRenamed,
                CollectionChange::Moved => Self::CollectionMoved,
                CollectionChange::Deleted => Self::CollectionDeleted,
                CollectionChange::MediaAdded => Self::CollectionMediaAdded,
                CollectionChange::MediaRemoved => Self::CollectionMediaRemoved,
                CollectionChange::QuickAdded => Self::QuickCollectionAdded,
                CollectionChange::QuickRemoved => Self::QuickCollectionRemoved,
                CollectionChange::QuickCleared => Self::QuickCollectionCleared,
                CollectionChange::QuickSaved => Self::QuickCollectionSaved,
                CollectionChange::RulesChanged => Self::SmartRulesChanged,
            },
            EditChange::FileRelocated => Self::FileRelocated,
            EditChange::FolderRelocated => Self::FolderRelocated,
            EditChange::MediaMoved => Self::MediaMoved,
            EditChange::MediaRenamed => Self::MediaRenamed,
            EditChange::Stack(change) => match change {
                StackChange::Stacked => Self::Stacked,
                StackChange::Unstacked => Self::Unstacked,
                StackChange::CoverChanged => Self::StackCoverChanged,
                StackChange::Removed => Self::RemovedFromStack,
                StackChange::PairsStacked => Self::PairsStacked,
                StackChange::BurstsStacked => Self::BurstsStacked,
            },
            EditChange::KeywordTree(change) => match change {
                KeywordTreeChange::Renamed => Self::KeywordRenamed,
                KeywordTreeChange::Moved => Self::KeywordMoved,
                KeywordTreeChange::Merged => Self::KeywordsMerged,
                KeywordTreeChange::Deleted => Self::KeywordsDeleted,
                KeywordTreeChange::Export(include) => Self::KeywordExport { include },
                KeywordTreeChange::Imported(count) => Self::KeywordsImported {
                    count: u32::try_from(count).unwrap_or(u32::MAX),
                },
            },
        }
    }
}

impl From<RotationDirection> for RotationDirectionDto {
    fn from(direction: RotationDirection) -> Self {
        match direction {
            RotationDirection::Left => Self::Left,
            RotationDirection::Right => Self::Right,
        }
    }
}

impl From<RotationDirectionDto> for RotationDirection {
    fn from(direction: RotationDirectionDto) -> Self {
        match direction {
            RotationDirectionDto::Left => Self::Left,
            RotationDirectionDto::Right => Self::Right,
        }
    }
}

impl From<EditSummary> for EditSummaryDto {
    fn from(summary: EditSummary) -> Self {
        Self {
            change: EditChangeDto::from(summary.change),
            count: u32::try_from(summary.count).unwrap_or(u32::MAX),
        }
    }
}

impl From<Flag> for FlagDto {
    fn from(flag: Flag) -> Self {
        match flag {
            Flag::Picked => Self::Picked,
            Flag::Rejected => Self::Rejected,
        }
    }
}

impl From<FlagDto> for Flag {
    fn from(flag: FlagDto) -> Self {
        match flag {
            FlagDto::Picked => Self::Picked,
            FlagDto::Rejected => Self::Rejected,
        }
    }
}

impl From<ColorLabel> for ColorLabelDto {
    fn from(color: ColorLabel) -> Self {
        match color {
            ColorLabel::Red => Self::Red,
            ColorLabel::Yellow => Self::Yellow,
            ColorLabel::Green => Self::Green,
            ColorLabel::Blue => Self::Blue,
            ColorLabel::Purple => Self::Purple,
        }
    }
}

impl From<ColorLabelDto> for ColorLabel {
    fn from(color: ColorLabelDto) -> Self {
        match color {
            ColorLabelDto::Red => Self::Red,
            ColorLabelDto::Yellow => Self::Yellow,
            ColorLabelDto::Green => Self::Green,
            ColorLabelDto::Blue => Self::Blue,
            ColorLabelDto::Purple => Self::Purple,
        }
    }
}
