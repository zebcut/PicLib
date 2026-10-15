//! Modèles de métadonnées : les lire, en enregistrer, en
//! supprimer (réglages hors historique), et en appliquer un aux photos
//! sélectionnées, de façon annulable.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use super::keywords::keyword_id_dto;
use super::metadata::MetadataFieldDto;
use crate::catalog::repositories::metadata_templates::{FieldAction, MetadataTemplate};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{KeywordEntryError, MetadataTemplateId};
use crate::edits::{Edit, History};
use crate::keywords::KeywordError;
use crate::metadata_templates::{self, FieldChoice, TemplateError, TemplateInput};

/// Un modèle de métadonnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetadataTemplateDto {
    /// Identifiant.
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Champs remplis ou vidés, dans l'ordre du panneau.
    pub fields: Vec<TemplateFieldDto>,
    /// Mots-clés ajoutés.
    pub keyword_ids: Vec<u32>,
    /// Choisi au dernier import : présélectionné au suivant.
    pub used_at_import: bool,
}

/// Ce qu'un modèle fait d'un champ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateFieldDto {
    /// Champ.
    pub field: MetadataFieldDto,
    /// Valeur donnée ; `null` : le champ est vidé.
    pub value: Option<String>,
}

/// Contenu d'un modèle, tel que rempli dans sa fenêtre.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInputDto {
    /// Nom.
    pub name: String,
    /// Champs à remplir (`value`) ou à vider (`null`) ; les autres ne
    /// changent pas.
    pub fields: Vec<TemplateFieldDto>,
    /// Mots-clés à ajouter, saisis comme dans le panneau Mots-clés.
    pub keywords: String,
}

/// Modèle refusé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MetadataTemplateFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// Le modèle n'existe plus.
    NotFound,
    /// Nom vide.
    EmptyName,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Un autre modèle porte déjà ce nom.
    NameTaken {
        /// Nom concerné.
        name: String,
    },
    /// « Remplir » sans valeur.
    EmptyValue {
        /// Champ concerné.
        field: MetadataFieldDto,
    },
    /// Valeur de plus de 10 000 caractères.
    ValueTooLong {
        /// Champ concerné.
        field: MetadataFieldDto,
    },
    /// Un nom seul désigne plusieurs mots-clés, aucun à la racine.
    AmbiguousKeyword {
        /// Nom concerné.
        name: String,
    },
    /// Mot-clé de plus de 200 caractères.
    KeywordTooLong,
    /// Mot-clé contenant « | ».
    ForbiddenCharacter {
        /// Nom concerné.
        name: String,
    },
}

/// Tous les modèles, dans l'ordre de leur nom.
///
/// # Errors
///
/// [`MetadataTemplateFailure::CatalogUnavailable`] si le catalogue est
/// illisible.
#[tauri::command]
#[specta::specta]
pub async fn get_metadata_templates(
    catalog: State<'_, Mutex<Catalog>>,
) -> Result<Vec<MetadataTemplateDto>, MetadataTemplateFailure> {
    let catalog = lock(&catalog);
    let templates = metadata_templates::templates(catalog.connection())
        .map_err(|error| catalog_failure(&error))?;
    Ok(templates
        .into_iter()
        .map(MetadataTemplateDto::from)
        .collect())
}

/// Enregistre un modèle (nouveau si `template_id` est `null`) et renvoie
/// son identifiant. Les mots-clés saisis qui manquent sont créés.
///
/// # Errors
///
/// Voir [`MetadataTemplateFailure`] ; rien n'est enregistré ni créé en cas
/// d'échec.
#[tauri::command]
#[specta::specta]
pub async fn save_metadata_template(
    catalog: State<'_, Mutex<Catalog>>,
    template_id: Option<u32>,
    template: TemplateInputDto,
) -> Result<u32, MetadataTemplateFailure> {
    let input = TemplateInput {
        name: template.name,
        fields: template
            .fields
            .into_iter()
            .map(|field| {
                (
                    field.field.into(),
                    field.value.map_or(FieldChoice::Clear, FieldChoice::Fill),
                )
            })
            .collect(),
        keywords: template.keywords,
    };
    let mut catalog = lock(&catalog);
    let transaction = catalog
        .transaction()
        .map_err(|error| catalog_failure(&error))?;
    let id = metadata_templates::save(&transaction, template_id.map(template_id_of), &input)
        .map_err(template_failure)?;
    transaction
        .commit()
        .map_err(|error| catalog_failure(&error.into()))?;
    Ok(template_id_dto(id))
}

/// Supprime un modèle ; ses mots-clés restent.
///
/// # Errors
///
/// Voir [`MetadataTemplateFailure`].
#[tauri::command]
#[specta::specta]
pub async fn delete_metadata_template(
    catalog: State<'_, Mutex<Catalog>>,
    template_id: u32,
) -> Result<(), MetadataTemplateFailure> {
    let catalog = lock(&catalog);
    metadata_templates::delete(catalog.connection(), template_id_of(template_id))
        .map_err(template_failure)
}

/// Applique un modèle aux médias indiqués, de façon annulable : ses champs
/// sont remplis ou vidés, ses mots-clés ajoutés.
///
/// # Errors
///
/// Voir [`EditFailure`] (`InvalidValue` si le modèle n'existe plus) ; rien
/// n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn apply_metadata_template(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    template_id: u32,
    media_ids: Vec<u32>,
) -> Result<EditOutcome, EditFailure> {
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let template = metadata_templates::template(catalog.connection(), template_id_of(template_id))
        .map_err(|error| match error {
            TemplateError::Catalog(error) => failure(&error),
            _ => EditFailure::InvalidValue,
        })?;
    let summary = Edit::template(&catalog, &template, &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

fn template_id_of(id: u32) -> MetadataTemplateId {
    MetadataTemplateId::new(i64::from(id))
}

fn template_id_dto(id: MetadataTemplateId) -> u32 {
    u32::try_from(id.get()).unwrap_or(u32::MAX)
}

fn catalog_failure(error: &CatalogError) -> MetadataTemplateFailure {
    failure(error);
    MetadataTemplateFailure::CatalogUnavailable
}

fn template_failure(error: TemplateError) -> MetadataTemplateFailure {
    match error {
        TemplateError::EmptyName => MetadataTemplateFailure::EmptyName,
        TemplateError::TooLong => MetadataTemplateFailure::TooLong,
        TemplateError::NameTaken(name) => MetadataTemplateFailure::NameTaken { name },
        TemplateError::NotFound => MetadataTemplateFailure::NotFound,
        TemplateError::EmptyValue(field) => MetadataTemplateFailure::EmptyValue {
            field: field.into(),
        },
        TemplateError::ValueTooLong(field) => MetadataTemplateFailure::ValueTooLong {
            field: field.into(),
        },
        TemplateError::Keywords(KeywordError::Ambiguous(name)) => {
            MetadataTemplateFailure::AmbiguousKeyword { name }
        }
        TemplateError::Keywords(KeywordError::Entry(KeywordEntryError::TooLong(_))) => {
            MetadataTemplateFailure::KeywordTooLong
        }
        TemplateError::Keywords(KeywordError::Entry(KeywordEntryError::ForbiddenCharacter(
            name,
        ))) => MetadataTemplateFailure::ForbiddenCharacter { name },
        TemplateError::Keywords(KeywordError::Catalog(error)) | TemplateError::Catalog(error) => {
            catalog_failure(&error)
        }
    }
}

impl From<MetadataTemplate> for MetadataTemplateDto {
    fn from(template: MetadataTemplate) -> Self {
        Self {
            id: template_id_dto(template.id),
            name: template.name,
            fields: template
                .fields
                .into_iter()
                .map(|(field, action)| TemplateFieldDto {
                    field: field.into(),
                    value: match action {
                        FieldAction::Fill(value) => Some(value),
                        FieldAction::Clear => None,
                    },
                })
                .collect(),
            keyword_ids: template.keywords.into_iter().map(keyword_id_dto).collect(),
            used_at_import: template.used_at_import,
        }
    }
}
