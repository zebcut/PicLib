//! Types échangés avec l'interface pour les collections.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::edits::EditOutcome;
use crate::catalog::repositories::collections::CollectionKind;
use crate::catalog::repositories::smart_rules::SmartRule;
use crate::collections::CollectionError;

/// Collection, ou dossier de collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CollectionKindDto {
    /// Collection de photos.
    Collection,
    /// Dossier de collections.
    Folder,
}

impl From<CollectionKindDto> for CollectionKind {
    fn from(kind: CollectionKindDto) -> Self {
        match kind {
            CollectionKindDto::Collection => Self::Collection,
            CollectionKindDto::Folder => Self::Folder,
        }
    }
}

/// Une ligne du panneau Collections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    /// Identifiant.
    pub id: u32,
    /// Dossier parent ; `null` à la racine.
    pub parent_id: Option<u32>,
    /// Collection ou dossier.
    pub kind: CollectionKindDto,
    /// Nom.
    pub name: String,
    /// Photos (chacune une fois pour un dossier).
    #[specta(type = u32)]
    pub media_count: u64,
    /// Règles d'une collection dynamique ; `null` pour une collection
    /// manuelle ou un dossier.
    pub rules: Option<SmartRule>,
}

/// Changement demandé.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CollectionOperation {
    /// Créer une collection ou un dossier ; une collection reçoit aussitôt
    /// `mediaIds`.
    #[serde(rename_all = "camelCase")]
    Create {
        /// Dossier parent ; `null` à la racine.
        parent_id: Option<u32>,
        /// Collection ou dossier.
        collection_kind: CollectionKindDto,
        /// Nom.
        name: String,
        /// Photos à y mettre.
        media_ids: Vec<u32>,
    },
    /// Créer une collection dynamique.
    #[serde(rename_all = "camelCase")]
    CreateSmart {
        /// Dossier parent ; `null` à la racine.
        parent_id: Option<u32>,
        /// Nom.
        name: String,
        /// Règles.
        rules: SmartRule,
    },
    /// Remplacer les règles d'une collection dynamique.
    #[serde(rename_all = "camelCase")]
    SetRules {
        /// Collection dynamique.
        collection_id: u32,
        /// Nouvelles règles.
        rules: SmartRule,
    },
    /// Renommer.
    #[serde(rename_all = "camelCase")]
    Rename {
        /// Collection ou dossier.
        collection_id: u32,
        /// Nouveau nom.
        name: String,
    },
    /// Ranger sous un autre dossier (`null` : à la racine).
    #[serde(rename_all = "camelCase")]
    Move {
        /// Collection ou dossier.
        collection_id: u32,
        /// Nouveau dossier parent.
        parent_id: Option<u32>,
    },
    /// Supprimer (avec son contenu, pour un dossier).
    #[serde(rename_all = "camelCase")]
    Delete {
        /// Collection ou dossier.
        collection_id: u32,
    },
    /// Mettre des photos dans une collection.
    #[serde(rename_all = "camelCase")]
    AddMedia {
        /// Collection.
        collection_id: u32,
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Retirer des photos d'une collection.
    #[serde(rename_all = "camelCase")]
    RemoveMedia {
        /// Collection.
        collection_id: u32,
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Touche B : ajouter à la collection cible, ou en retirer si toutes y
    /// sont déjà.
    #[serde(rename_all = "camelCase")]
    ToggleInTarget {
        /// Photos.
        media_ids: Vec<u32>,
    },
    /// Vider la collection rapide.
    ClearQuick,
    /// Enregistrer la collection rapide comme collection, à la racine.
    #[serde(rename_all = "camelCase")]
    SaveQuick {
        /// Nom de la collection.
        name: String,
        /// Vider ensuite la collection rapide.
        clear: bool,
    },
}

/// Collection rapide et collection cible, pour le panneau Catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickCollectionDto {
    /// Photos de la collection rapide.
    #[specta(type = u32)]
    pub media_count: u64,
    /// Collection cible ; `null` : la collection rapide.
    pub target_id: Option<u32>,
}

/// Changement refusé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CollectionFailure {
    /// Nom vide.
    EmptyName,
    /// Nom trop long.
    TooLong,
    /// Un voisin porte déjà ce nom.
    NameTaken,
    /// La collection ou le dossier n'existe plus.
    NotFound,
    /// Mauvaise sorte : ranger dans une collection, ou mettre des photos
    /// dans un dossier.
    WrongKind,
    /// Un dossier dans l'un de ses propres sous-dossiers.
    IntoItself,
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
}

impl From<CollectionError> for CollectionFailure {
    fn from(error: CollectionError) -> Self {
        match error {
            CollectionError::EmptyName => Self::EmptyName,
            CollectionError::TooLong => Self::TooLong,
            CollectionError::NameTaken(_) => Self::NameTaken,
            CollectionError::NotFound => Self::NotFound,
            CollectionError::WrongKind => Self::WrongKind,
            CollectionError::IntoItself => Self::IntoItself,
            CollectionError::Catalog(error) => {
                tracing::error!(%error, "cannot change the collections");
                Self::CatalogUnavailable
            }
        }
    }
}

/// Résultat d'un changement : la collection créée (s'il y a lieu), et
/// l'état de l'historique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionChangeDto {
    /// Collection ou dossier créé.
    pub created_id: Option<u32>,
    /// Résultat (pour le menu Édition).
    pub outcome: EditOutcome,
}

/// Ce qu'emporterait une suppression, pour la confirmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDeletionDto {
    /// Collections supprimées.
    pub collections: u32,
    /// Dossiers de collections supprimés.
    pub folders: u32,
}
