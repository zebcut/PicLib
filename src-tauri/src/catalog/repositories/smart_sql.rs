//! Traduction des règles des collections dynamiques (`smart_rules`) en
//! condition SQL sur `media`, avec ses paramètres.

use std::rc::Rc;

use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::vtab::array::Array;
use rusqlite::{Connection, ToSql};

use super::collections;
use super::library_query::LibrarySource;
use super::smart_rules::{
    SmartColor, SmartCombine, SmartDateComparison, SmartFlag, SmartKind, SmartRatingComparison,
    SmartRule, SmartTextField, SmartTextRule, parse,
};
use super::text_search::TextSearch;
use crate::catalog::CatalogError;
use crate::catalog::sql::{color_label_to_text, media_format_to_text};
use crate::domain::{ColorLabel, MediaFormat, Rating, TextField, TextRule};

/// Profondeur maximale des groupes : borne une règle abîmée ou malveillante.
const MAX_DEPTH: usize = 16;

/// Valeur d'un paramètre d'une condition.
pub(super) enum SmartValue {
    /// Texte (date, texte cherché).
    Text(String),
    /// Entier (identifiant).
    Integer(i64),
    /// Liste d'identifiants (médias retenus par une règle de texte).
    Ids(Array),
}

impl ToSql for SmartValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Self::Text(text) => text.to_sql(),
            Self::Integer(value) => value.to_sql(),
            Self::Ids(ids) => ids.to_sql(),
        }
    }
}

/// Traduit des règles en condition SQL, en accumulant les paramètres
/// (`:smart_0`, `:smart_1`…).
pub(super) struct SmartSql<'c> {
    connection: &'c Connection,
    /// Paramètres et leurs valeurs.
    pub(super) params: Vec<(String, SmartValue)>,
}

impl<'c> SmartSql<'c> {
    pub(super) fn new(connection: &'c Connection) -> Self {
        Self {
            connection,
            params: Vec::new(),
        }
    }

    fn parameter(&mut self, value: SmartValue) -> String {
        let name = format!(":smart_{}", self.params.len());
        self.params.push((name.clone(), value));
        name
    }

    /// Condition SQL de la règle (et de ses sous-règles).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible (règle de texte).
    pub(super) fn condition(&mut self, rule: &SmartRule) -> Result<String, CatalogError> {
        self.condition_at(rule, 0)
    }

    fn condition_at(&mut self, rule: &SmartRule, depth: usize) -> Result<String, CatalogError> {
        Ok(match rule {
            SmartRule::Group { combine, rules } => {
                if depth >= MAX_DEPTH {
                    return Ok("0".to_owned());
                }
                let parts = rules
                    .iter()
                    .map(|inner| self.condition_at(inner, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                match (combine, parts.is_empty()) {
                    // Un groupe vide n'écarte rien (« toutes ») ou ne retient
                    // rien (« au moins une »).
                    (SmartCombine::All, true) => "1".to_owned(),
                    (SmartCombine::Any, true) => "0".to_owned(),
                    (SmartCombine::All, false) => format!("({})", parts.join(" AND ")),
                    (SmartCombine::Any, false) => format!("({})", parts.join(" OR ")),
                }
            }
            SmartRule::Rating { comparison, stars } => {
                let operator = match comparison {
                    SmartRatingComparison::AtLeast => ">=",
                    SmartRatingComparison::Equal => "=",
                    SmartRatingComparison::AtMost => "<=",
                };
                format!("media.rating {operator} {}", (*stars).min(Rating::MAX))
            }
            SmartRule::Flag { flag } => match flag {
                SmartFlag::Picked => "media.flag = 'picked'",
                SmartFlag::Unflagged => "media.flag IS NULL",
                SmartFlag::Rejected => "media.flag = 'rejected'",
            }
            .to_owned(),
            SmartRule::Color { color } => match color_label(*color) {
                Some(label) => format!("media.color_label = '{}'", color_label_to_text(label)),
                None => "media.color_label IS NULL".to_owned(),
            },
            SmartRule::Keyword {
                keyword_id,
                include_children,
            } => self.keyword(*keyword_id, *include_children),
            SmartRule::CaptureDate { comparison, date } => {
                self.date_condition("substr(media.capture_time, 1, 10)", *comparison, date)
            }
            SmartRule::ImportDate { comparison, date } => self.date_condition(
                "(SELECT date(imports.started_at / 1000, 'unixepoch', 'localtime')
                  FROM imports WHERE imports.id = media.import_id)",
                *comparison,
                date,
            ),
            SmartRule::Camera { text } => self.contains(
                "coalesce(media.camera_model, '') || ' ' || coalesce(media.camera_make, '')",
                text,
            ),
            SmartRule::Lens { text } => self.contains("coalesce(media.lens, '')", text),
            SmartRule::FileKind { file_kind } => file_kind_condition(*file_kind),
            SmartRule::Folder {
                folder_id,
                include_subfolders,
            } => self.folder(*folder_id, *include_subfolders),
            SmartRule::Text { field, rule, text } => self.text(*field, *rule, text)?,
            SmartRule::Gps { present } => if *present {
                "media.gps_latitude IS NOT NULL"
            } else {
                "media.gps_latitude IS NULL"
            }
            .to_owned(),
            SmartRule::Stack { stacked } => {
                if *stacked {
                    super::stacks::IN_STACK.to_owned()
                } else {
                    format!("NOT {}", super::stacks::IN_STACK)
                }
            }
        })
    }

    /// Photos qui portent le mot-clé, et ses sous-mots-clés si demandé.
    fn keyword(&mut self, keyword_id: u32, include_children: bool) -> String {
        let id = self.parameter(SmartValue::Integer(i64::from(keyword_id)));
        if include_children {
            format!(
                "media.id IN (SELECT media_keywords.media_id FROM media_keywords
                 WHERE media_keywords.keyword_id IN (
                     WITH RECURSIVE branch(id) AS (
                         SELECT {id}
                         UNION
                         SELECT keywords.id FROM keywords
                         JOIN branch ON coalesce(keywords.parent_id, 0) = branch.id
                     )
                     SELECT id FROM branch))"
            )
        } else {
            format!("media.id IN (SELECT media_id FROM media_keywords WHERE keyword_id = {id})")
        }
    }

    /// Photos du dossier, et de ses sous-dossiers si demandé.
    fn folder(&mut self, folder_id: u32, include_subfolders: bool) -> String {
        let id = self.parameter(SmartValue::Integer(i64::from(folder_id)));
        if include_subfolders {
            format!(
                "media.folder_id IN (
                     WITH RECURSIVE branch(id) AS (
                         SELECT {id}
                         UNION
                         SELECT folders.id FROM folders
                         JOIN branch ON folders.parent_id = branch.id
                     )
                     SELECT id FROM branch)"
            )
        } else {
            format!("media.folder_id = {id}")
        }
    }

    /// Photos retenues par une règle de texte, cherchées comme le filtre
    /// Texte, puis passées en liste.
    fn text(
        &mut self,
        field: SmartTextField,
        rule: SmartTextRule,
        text: &str,
    ) -> Result<String, CatalogError> {
        let search = TextSearch {
            field: match field {
                SmartTextField::FileName => TextField::FileName,
                SmartTextField::Title => TextField::Title,
                SmartTextField::Caption => TextField::Caption,
                SmartTextField::Place => TextField::Place,
            },
            rule: text_rule(rule),
            text: text.to_owned(),
        };
        if !search.has_terms() {
            return Ok("1".to_owned());
        }
        let ids = search.matching_ids(self.connection, "1", None)?;
        let ids = Rc::new(ids.into_iter().map(Value::from).collect());
        let name = self.parameter(SmartValue::Ids(ids));
        Ok(format!("media.id IN rarray({name})"))
    }

    /// Comparaison d'un jour ; une date mal formée ne retient rien.
    fn date_condition(
        &mut self,
        expression: &str,
        comparison: SmartDateComparison,
        date: &str,
    ) -> String {
        if !is_day(date) {
            return "0".to_owned();
        }
        let day = self.parameter(SmartValue::Text(date.to_owned()));
        let operator = match comparison {
            SmartDateComparison::Before => "<",
            SmartDateComparison::On => "=",
            SmartDateComparison::After => ">",
        };
        format!("{expression} {operator} {day}")
    }

    /// Le texte de `expression` contient `text`, sans tenir compte des
    /// majuscules (lettres non accentuées) ; un texte vide ne retient rien de
    /// plus.
    fn contains(&mut self, expression: &str, text: &str) -> String {
        let text = text.trim();
        if text.is_empty() {
            return "1".to_owned();
        }
        let needle = self.parameter(SmartValue::Text(text.to_owned()));
        format!("instr(lower({expression}), lower({needle})) > 0")
    }
}

/// Type de fichier.
fn file_kind_condition(kind: SmartKind) -> String {
    match kind {
        SmartKind::Photo => "media.kind = 'photo'".to_owned(),
        SmartKind::Video => "media.kind = 'video'".to_owned(),
        SmartKind::Raw => {
            let formats: Vec<String> = MediaFormat::ALL
                .into_iter()
                .filter(|format| format.is_raw())
                .map(|format| format!("'{}'", media_format_to_text(format)))
                .collect();
            format!("media.format IN ({})", formats.join(", "))
        }
    }
}

/// Vrai pour un jour au format `AAAA-MM-JJ`.
fn is_day(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn color_label(color: SmartColor) -> Option<ColorLabel> {
    match color {
        SmartColor::Red => Some(ColorLabel::Red),
        SmartColor::Yellow => Some(ColorLabel::Yellow),
        SmartColor::Green => Some(ColorLabel::Green),
        SmartColor::Blue => Some(ColorLabel::Blue),
        SmartColor::Purple => Some(ColorLabel::Purple),
        SmartColor::None => None,
    }
}

fn text_rule(rule: SmartTextRule) -> TextRule {
    match rule {
        SmartTextRule::Contains => TextRule::Contains,
        SmartTextRule::ContainsAll => TextRule::ContainsAll,
        SmartTextRule::ContainsWords => TextRule::ContainsWords,
        SmartTextRule::DoesNotContain => TextRule::DoesNotContain,
        SmartTextRule::StartsWith => TextRule::StartsWith,
        SmartTextRule::EndsWith => TextRule::EndsWith,
    }
}

/// Condition de la source : pour une collection ou un dossier de
/// collections, les photos rangées, plus celles que retiennent les règles
/// des collections dynamiques de sa branche, avec leurs paramètres.
pub(super) fn source_condition(
    connection: &Connection,
    source: LibrarySource,
    static_source: &str,
) -> Result<(String, Vec<(String, SmartValue)>), CatalogError> {
    let LibrarySource::Collection(id) = source else {
        return Ok((static_source.to_owned(), Vec::new()));
    };
    let rules = collections::smart_rules_in_branch(connection, id)?;
    if rules.is_empty() {
        return Ok((static_source.to_owned(), Vec::new()));
    }
    let mut sql = SmartSql::new(connection);
    let mut parts = vec![format!("({static_source})")];
    for rule in rules.iter().filter_map(|rules| parse(rules)) {
        parts.push(sql.condition(&rule)?);
    }
    Ok((parts.join(" OR "), sql.params))
}
