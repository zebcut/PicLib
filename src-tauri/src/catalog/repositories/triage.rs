//! Lecture et écriture des attributs de tri (marqueur, note, couleur) d'un
//! ensemble de médias.

use rusqlite::{Connection, params_from_iter, types::Value};

use crate::catalog::CatalogError;
use crate::catalog::sql::{
    IDS_PER_QUERY, color_label_from_text, color_label_to_text, flag_from_text, flag_to_text,
    placeholders, rating_from_sql,
};
use crate::domain::{ColorLabel, Flag, MediaId, Rating};

/// Attribut de tri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriageAttribute {
    /// Marqueur.
    Flag,
    /// Note.
    Rating,
    /// Étiquette de couleur.
    ColorLabel,
}

impl TriageAttribute {
    fn column(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Rating => "rating",
            Self::ColorLabel => "color_label",
        }
    }
}

/// Valeur d'un attribut de tri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriageValue {
    /// Marqueur (ou aucun).
    Flag(Option<Flag>),
    /// Note.
    Rating(Rating),
    /// Étiquette de couleur (ou aucune).
    ColorLabel(Option<ColorLabel>),
}

impl TriageValue {
    /// Attribut concerné.
    #[must_use]
    pub fn attribute(self) -> TriageAttribute {
        match self {
            Self::Flag(_) => TriageAttribute::Flag,
            Self::Rating(_) => TriageAttribute::Rating,
            Self::ColorLabel(_) => TriageAttribute::ColorLabel,
        }
    }

    fn to_sql_value(self) -> Value {
        match self {
            Self::Flag(flag) => {
                flag.map_or(Value::Null, |flag| Value::Text(flag_to_text(flag).into()))
            }
            Self::Rating(rating) => Value::Integer(i64::from(rating.stars())),
            Self::ColorLabel(color) => color.map_or(Value::Null, |color| {
                Value::Text(color_label_to_text(color).into())
            }),
        }
    }

    fn from_sql_value(attribute: TriageAttribute, value: Value) -> Result<Self, CatalogError> {
        Ok(match (attribute, value) {
            (TriageAttribute::Flag, Value::Null) => Self::Flag(None),
            (TriageAttribute::Flag, Value::Text(text)) => Self::Flag(Some(flag_from_text(&text)?)),
            (TriageAttribute::Rating, Value::Integer(stars)) => {
                Self::Rating(rating_from_sql(stars)?)
            }
            (TriageAttribute::ColorLabel, Value::Null) => Self::ColorLabel(None),
            (TriageAttribute::ColorLabel, Value::Text(text)) => {
                Self::ColorLabel(Some(color_label_from_text(&text)?))
            }
            (attribute, value) => {
                return Err(CatalogError::InvalidValue {
                    column: attribute.column(),
                    value: format!("{value:?}"),
                });
            }
        })
    }
}

/// Valeur actuelle d'un attribut pour chaque média existant parmi `ids`
/// (les identifiants inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn read(
    connection: &Connection,
    attribute: TriageAttribute,
    ids: &[MediaId],
) -> Result<Vec<(MediaId, TriageValue)>, CatalogError> {
    let mut values = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT id, {} FROM media WHERE id IN ({})",
            attribute.column(),
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = params_from_iter(chunk.iter().map(|id| id.get()));
        let rows = statement.query_map(parameters, |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Value>(1)?))
        })?;
        for row in rows {
            let (id, value) = row?;
            values.push((
                MediaId::new(id),
                TriageValue::from_sql_value(attribute, value)?,
            ));
        }
    }
    Ok(values)
}

/// Donne la valeur `value` aux médias `ids` ; renvoie le nombre de médias
/// modifiés. À appeler dans une transaction pour un ensemble cohérent.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn write(
    connection: &Connection,
    value: TriageValue,
    ids: &[MediaId],
) -> Result<usize, CatalogError> {
    let column = value.attribute().column();
    let sql_value = value.to_sql_value();
    let mut changed = 0;
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "UPDATE media SET {column} = ?1 WHERE id IN ({})",
            placeholders(2, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let parameters = std::iter::once(sql_value.clone())
            .chain(chunk.iter().map(|id| Value::Integer(id.get())));
        changed += statement.execute(params_from_iter(parameters))?;
    }
    Ok(changed)
}
