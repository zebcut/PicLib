//! Modèles de métadonnées (migrations 15 et 16) : nom, champs à remplir
//! ou à vider, mots-clés à ajouter, et celui choisi au dernier import. À appeler dans une transaction pour les
//! écritures en plusieurs requêtes.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use super::metadata::column;
use crate::catalog::CatalogError;
use crate::domain::{KeywordId, MetadataField, MetadataTemplateId, keyword_name_key};

/// Ce qu'un modèle fait d'un champ (un champ absent n'est pas changé).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldAction {
    /// Donne cette valeur au champ.
    Fill(String),
    /// Vide le champ.
    Clear,
}

/// Un modèle de métadonnées.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataTemplate {
    /// Identifiant.
    pub id: MetadataTemplateId,
    /// Nom.
    pub name: String,
    /// Champs changés, dans l'ordre du panneau Métadonnées.
    pub fields: Vec<(MetadataField, FieldAction)>,
    /// Mots-clés à ajouter (seulement ceux qui existent encore).
    pub keywords: Vec<KeywordId>,
    /// Choisi au dernier import : présélectionné au suivant.
    pub used_at_import: bool,
}

/// Tous les modèles, dans l'ordre de leur nom.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn all(connection: &Connection) -> Result<Vec<MetadataTemplate>, CatalogError> {
    let mut templates: Vec<MetadataTemplate> = connection
        .prepare_cached(
            "SELECT id, name, used_at_import FROM metadata_templates ORDER BY name_key",
        )?
        .query_map([], |row| {
            Ok(MetadataTemplate {
                id: MetadataTemplateId::new(row.get(0)?),
                name: row.get(1)?,
                fields: Vec::new(),
                keywords: Vec::new(),
                used_at_import: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let position: HashMap<MetadataTemplateId, usize> = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.id, index))
        .collect();

    let mut fields = connection
        .prepare_cached("SELECT template_id, field, value FROM metadata_template_fields")?;
    let rows = fields.query_map([], |row| {
        Ok((
            MetadataTemplateId::new(row.get(0)?),
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (id, code, value) = row?;
        let field = MetadataField::ALL
            .into_iter()
            .find(|field| column(*field) == code);
        if let Some((field, index)) = field.zip(position.get(&id)) {
            let action = value.map_or(FieldAction::Clear, FieldAction::Fill);
            templates[*index].fields.push((field, action));
        }
    }
    for template in &mut templates {
        template
            .fields
            .sort_by_key(|(field, _)| MetadataField::ALL.iter().position(|other| other == field));
    }

    // Seuls les mots-clés qui existent encore.
    let mut keywords = connection.prepare_cached(
        "SELECT template_id, keyword_id FROM metadata_template_keywords
         JOIN keywords ON keywords.id = metadata_template_keywords.keyword_id
         ORDER BY keywords.name_key",
    )?;
    let rows = keywords.query_map([], |row| {
        Ok((
            MetadataTemplateId::new(row.get(0)?),
            KeywordId::new(row.get(1)?),
        ))
    })?;
    for row in rows {
        let (id, keyword) = row?;
        if let Some(index) = position.get(&id) {
            templates[*index].keywords.push(keyword);
        }
    }
    Ok(templates)
}

/// Modèle qui porte ce nom (sans tenir compte des majuscules).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn find_by_name(
    connection: &Connection,
    name: &str,
) -> Result<Option<MetadataTemplateId>, CatalogError> {
    let id = connection
        .prepare_cached("SELECT id FROM metadata_templates WHERE name_key = ?1")?
        .query_row([keyword_name_key(name)], |row| row.get(0))
        .optional()?;
    Ok(id.map(MetadataTemplateId::new))
}

/// Vrai si le modèle existe.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn exists(connection: &Connection, id: MetadataTemplateId) -> Result<bool, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT 1 FROM metadata_templates WHERE id = ?1")?
        .query_row([id.get()], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Crée un modèle vide.
///
/// # Errors
///
/// Échoue si le nom est déjà pris, ou si la base ne peut pas être écrite.
pub fn create(connection: &Connection, name: &str) -> Result<MetadataTemplateId, CatalogError> {
    connection
        .prepare_cached("INSERT INTO metadata_templates (name, name_key) VALUES (?1, ?2)")?
        .execute(params![name, keyword_name_key(name)])?;
    Ok(MetadataTemplateId::new(connection.last_insert_rowid()))
}

/// Remplace le nom, les champs et les mots-clés d'un modèle.
///
/// # Errors
///
/// Échoue si le nom est déjà pris, ou si la base ne peut pas être écrite.
pub fn replace(
    connection: &Connection,
    id: MetadataTemplateId,
    name: &str,
    fields: &[(MetadataField, FieldAction)],
    keywords: &[KeywordId],
) -> Result<(), CatalogError> {
    connection
        .prepare_cached("UPDATE metadata_templates SET name = ?2, name_key = ?3 WHERE id = ?1")?
        .execute(params![id.get(), name, keyword_name_key(name)])?;
    connection
        .prepare_cached("DELETE FROM metadata_template_fields WHERE template_id = ?1")?
        .execute([id.get()])?;
    let mut insert_field = connection.prepare_cached(
        "INSERT INTO metadata_template_fields (template_id, field, value) VALUES (?1, ?2, ?3)",
    )?;
    for (field, action) in fields {
        let value = match action {
            FieldAction::Fill(value) => Some(value.as_str()),
            FieldAction::Clear => None,
        };
        insert_field.execute(params![id.get(), column(*field), value])?;
    }
    connection
        .prepare_cached("DELETE FROM metadata_template_keywords WHERE template_id = ?1")?
        .execute([id.get()])?;
    let mut insert_keyword = connection.prepare_cached(
        "INSERT OR IGNORE INTO metadata_template_keywords (template_id, keyword_id)
         VALUES (?1, ?2)",
    )?;
    for keyword in keywords {
        insert_keyword.execute(params![id.get(), keyword.get()])?;
    }
    Ok(())
}

/// Supprime un modèle (les mots-clés restent).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn delete(connection: &Connection, id: MetadataTemplateId) -> Result<(), CatalogError> {
    connection
        .prepare_cached("DELETE FROM metadata_templates WHERE id = ?1")?
        .execute([id.get()])?;
    Ok(())
}

/// Retient le modèle choisi à l'import (`None` : aucun). À appeler dans une
/// transaction.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn set_used_at_import(
    connection: &Connection,
    id: Option<MetadataTemplateId>,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE metadata_templates SET used_at_import = 0 WHERE used_at_import = 1",
        )?
        .execute([])?;
    if let Some(id) = id {
        connection
            .prepare_cached("UPDATE metadata_templates SET used_at_import = 1 WHERE id = ?1")?
            .execute([id.get()])?;
    }
    Ok(())
}

/// Modèles qui contiennent le mot-clé `keyword`, et s'ils contiennent aussi
/// `other` (fusion de `keyword` dans `other`).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn templates_with_keyword(
    connection: &Connection,
    keyword: KeywordId,
    other: KeywordId,
) -> Result<Vec<(MetadataTemplateId, bool)>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT template_id,
                EXISTS (SELECT 1 FROM metadata_template_keywords AS both
                        WHERE both.template_id = mine.template_id AND both.keyword_id = ?2)
         FROM metadata_template_keywords AS mine WHERE keyword_id = ?1",
    )?;
    let rows = statement
        .query_map(params![keyword.get(), other.get()], |row| {
            Ok((MetadataTemplateId::new(row.get(0)?), row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Fait désigner `to` au lieu de `from` dans un modèle.
///
/// # Errors
///
/// Échoue si le modèle contient déjà `to`, ou si la base ne peut pas être
/// écrite.
pub fn replace_keyword(
    connection: &Connection,
    template: MetadataTemplateId,
    from: KeywordId,
    to: KeywordId,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE metadata_template_keywords SET keyword_id = ?3
             WHERE template_id = ?1 AND keyword_id = ?2",
        )?
        .execute(params![template.get(), from.get(), to.get()])?;
    Ok(())
}

/// Ajoute un mot-clé à un modèle (rien s'il y est déjà).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn add_keyword(
    connection: &Connection,
    template: MetadataTemplateId,
    keyword: KeywordId,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT OR IGNORE INTO metadata_template_keywords (template_id, keyword_id)
             VALUES (?1, ?2)",
        )?
        .execute(params![template.get(), keyword.get()])?;
    Ok(())
}

/// Retire un mot-clé d'un modèle.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn remove_keyword(
    connection: &Connection,
    template: MetadataTemplateId,
    keyword: KeywordId,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "DELETE FROM metadata_template_keywords WHERE template_id = ?1 AND keyword_id = ?2",
        )?
        .execute(params![template.get(), keyword.get()])?;
    Ok(())
}
