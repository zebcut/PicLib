//! Modèles de métadonnées : des réglages réutilisables (auteur,
//! copyright, lieu, mots-clés…) à poser d'un geste sur des photos.
//!
//! Un modèle est un réglage : le créer, le modifier ou le supprimer ne
//! passe pas par l'historique (Cmd+Z), comme les jeux de mots-clés. Son
//! application, elle, est une modification annulable (`Edit::template`),
//! sauf à l'import : elle fait alors partie de l'import, qui ne
//! s'annule pas.

use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use crate::catalog::CatalogError;
use crate::catalog::repositories::metadata_templates::{self, FieldAction, MetadataTemplate};
use crate::catalog::repositories::{keyword_usage, keywords, metadata};
use crate::domain::{
    MAX_KEYWORD_CHARS, MAX_METADATA_CHARS, MediaId, MetadataField, MetadataTemplateId, Timestamp,
    metadata_text,
};
use crate::keywords::{KeywordError, resolve_entry};

/// Modèle refusé.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// Nom vide.
    #[error("empty template name")]
    EmptyName,
    /// Nom trop long.
    #[error("template name too long")]
    TooLong,
    /// Un autre modèle porte déjà ce nom.
    #[error("template name {0:?} already used")]
    NameTaken(String),
    /// Le modèle n'existe pas (ou plus).
    #[error("template not found")]
    NotFound,
    /// « Remplir » sans valeur.
    #[error("no value to fill {0:?}")]
    EmptyValue(MetadataField),
    /// Valeur trop longue.
    #[error("value of {0:?} too long")]
    ValueTooLong(MetadataField),
    /// Les mots-clés saisis sont mal formés ou ambigus.
    #[error(transparent)]
    Keywords(KeywordError),
    /// Le catalogue n'a pas pu être lu ou écrit.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Ce qu'on demande à un champ du modèle (un champ absent n'est pas
/// changé).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldChoice {
    /// Remplir avec ce texte.
    Fill(String),
    /// Vider le champ.
    Clear,
}

/// Contenu d'un modèle, tel que rempli dans sa fenêtre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInput {
    /// Nom.
    pub name: String,
    /// Champs à remplir ou à vider.
    pub fields: Vec<(MetadataField, FieldChoice)>,
    /// Mots-clés à ajouter, saisis comme dans le panneau Mots-clés
    /// (`Famille, Lieux > France`) ; ceux qui manquent sont créés.
    pub keywords: String,
}

/// Tous les modèles, dans l'ordre de leur nom.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn templates(connection: &Connection) -> Result<Vec<MetadataTemplate>, CatalogError> {
    metadata_templates::all(connection)
}

/// Le modèle `id`.
///
/// # Errors
///
/// [`TemplateError::NotFound`] s'il n'existe pas.
pub fn template(
    connection: &Connection,
    id: MetadataTemplateId,
) -> Result<MetadataTemplate, TemplateError> {
    metadata_templates::all(connection)?
        .into_iter()
        .find(|template| template.id == id)
        .ok_or(TemplateError::NotFound)
}

/// Enregistre un modèle : nouveau si `id` est `None`, sinon remplace
/// celui-ci. Renvoie son identifiant.
///
/// À appeler dans une transaction : si le modèle est refusé, aucun mot-clé
/// saisi ne doit rester créé.
///
/// # Errors
///
/// Voir [`TemplateError`].
pub fn save(
    connection: &Connection,
    id: Option<MetadataTemplateId>,
    input: &TemplateInput,
) -> Result<MetadataTemplateId, TemplateError> {
    let name = parse_template_name(&input.name)?;
    if let Some(id) = id
        && !metadata_templates::exists(connection, id)?
    {
        return Err(TemplateError::NotFound);
    }
    if metadata_templates::find_by_name(connection, &name)?.is_some_and(|other| Some(other) != id) {
        return Err(TemplateError::NameTaken(name));
    }
    let fields = input
        .fields
        .iter()
        .map(|(field, choice)| Ok((*field, field_action(*field, choice)?)))
        .collect::<Result<Vec<_>, TemplateError>>()?;
    let keywords = resolve_entry(connection, &input.keywords).map_err(|error| match error {
        KeywordError::Catalog(error) => TemplateError::Catalog(error),
        other => TemplateError::Keywords(other),
    })?;
    let id = match id {
        Some(id) => id,
        None => metadata_templates::create(connection, &name)?,
    };
    metadata_templates::replace(connection, id, &name, &fields, &keywords)?;
    Ok(id)
}

/// Supprime un modèle (ses mots-clés restent).
///
/// # Errors
///
/// Voir [`TemplateError`].
pub fn delete(connection: &Connection, id: MetadataTemplateId) -> Result<(), TemplateError> {
    if !metadata_templates::exists(connection, id)? {
        return Err(TemplateError::NotFound);
    }
    Ok(metadata_templates::delete(connection, id)?)
}

/// Modèle choisi pour un import (`None` : aucun) : lu une fois, au
/// lancement, et retenu pour être présélectionné à l'import suivant. À
/// appeler dans une transaction.
///
/// # Errors
///
/// [`TemplateError::NotFound`] si le modèle n'existe plus.
pub fn choose_for_import(
    connection: &Connection,
    id: Option<MetadataTemplateId>,
) -> Result<Option<MetadataTemplate>, TemplateError> {
    let template = id.map(|id| template(connection, id)).transpose()?;
    metadata_templates::set_used_at_import(connection, id)?;
    Ok(template)
}

/// Pose `template` sur des médias qui viennent d'être ajoutés au catalogue
///(import), hors historique : ses champs sont remplis ou vidés,
/// ses mots-clés ajoutés. Il passe après ce que l'import a lu dans les
/// fichiers. Les mots-clés supprimés depuis la lecture du modèle sont
/// ignorés. À appeler dans la transaction qui ajoute ces médias.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn apply_to_new_media(
    connection: &Connection,
    template: &MetadataTemplate,
    ids: &[MediaId],
) -> Result<(), CatalogError> {
    if ids.is_empty() {
        return Ok(());
    }
    for (field, action) in &template.fields {
        let value = match action {
            FieldAction::Fill(value) => Some(value.as_str()),
            FieldAction::Clear => None,
        };
        metadata::write(connection, *field, value, ids)?;
    }
    let added = keywords::existing_keywords(connection, &template.keywords)?;
    if !added.is_empty() {
        let links: Vec<_> = ids
            .iter()
            .flat_map(|&media| added.iter().map(move |&keyword| (media, keyword)))
            .collect();
        keywords::link(connection, &links)?;
        keyword_usage::mark_used(connection, &added, Timestamp::now())?;
    }
    Ok(())
}

fn field_action(field: MetadataField, choice: &FieldChoice) -> Result<FieldAction, TemplateError> {
    match choice {
        FieldChoice::Clear => Ok(FieldAction::Clear),
        FieldChoice::Fill(text) => {
            if text.chars().count() > MAX_METADATA_CHARS {
                return Err(TemplateError::ValueTooLong(field));
            }
            metadata_text(text)
                .map(FieldAction::Fill)
                .ok_or(TemplateError::EmptyValue(field))
        }
    }
}

/// Nom d'un modèle : espaces autour ignorés, forme Unicode composée.
fn parse_template_name(name: &str) -> Result<String, TemplateError> {
    let name: String = name.trim().nfc().collect();
    if name.is_empty() {
        Err(TemplateError::EmptyName)
    } else if name.chars().count() > MAX_KEYWORD_CHARS {
        Err(TemplateError::TooLong)
    } else {
        Ok(name)
    }
}
