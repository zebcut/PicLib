//! Filtre Métadonnées de la grille : les colonnes du
//! navigateur, la valeur de chaque média pour une colonne, et la condition
//! SQL des valeurs choisies.
//!
//! Chaque valeur est une clé texte stable (« 2024-07-14 », « 2.8 »,
//! « red ») ; l'interface la met en forme selon la langue. Une valeur
//! absente (`None`) est « Inconnu ».

use rusqlite::Connection;

use super::library_query::LibraryQuery;
use crate::catalog::CatalogError;
use crate::catalog::sql::size_from_sql;

/// Critère d'une colonne du navigateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataColumn {
    /// Jour de prise de vue (`2024-07-14`) ; un choix peut être une année
    /// (`2024`) ou un mois (`2024-07`).
    Date,
    /// Modèle d'appareil (sa marque à défaut).
    Camera,
    /// Objectif.
    Lens,
    /// Étiquette de couleur.
    ColorLabel,
    /// Mot-clé posé (son identifiant).
    Keyword,
    /// Pays.
    Country,
    /// Région.
    Region,
    /// Ville.
    City,
    /// Lieu précis.
    Location,
    /// Format de fichier (`jpeg`, `nef`…).
    FileType,
    /// Sensibilité ISO.
    Iso,
    /// Ouverture, au dixième (`2.8`).
    Aperture,
    /// Vitesse, en secondes (`0.004`).
    ShutterSpeed,
    /// Focale, au millimètre (`50`).
    FocalLength,
    /// Flash déclenché ou non (`fired`, `notFired`).
    Flash,
    /// Auteur.
    Creator,
    /// Largeur × hauteur en pixels (`6000×4000`).
    Dimensions,
    /// Paysage, portrait ou carré, rotations comprises.
    Orientation,
}

/// Valeurs choisies dans une colonne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChoice {
    /// Critère de la colonne.
    pub column: MetadataColumn,
    /// Valeurs choisies ; `None` : « Inconnu ».
    pub values: Vec<Option<String>>,
}

/// Une valeur d'une colonne et son nombre de médias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnValue {
    /// Clé de la valeur ; `None` : « Inconnu ».
    pub value: Option<String>,
    /// Nom à afficher quand la clé n'en est pas un (le nom d'un mot-clé).
    pub label: Option<String>,
    /// Précision pour distinguer deux valeurs de même nom (le parent d'un
    /// mot-clé).
    pub detail: Option<String>,
    /// Nombre de médias.
    pub count: u64,
}

/// Texte vide ou fait d'espaces : valeur inconnue.
fn text(column: &str) -> String {
    format!("nullif(trim({column}), '')")
}

/// Rien quand la colonne numérique est vide, sinon `value`.
fn when_known(column: &str, value: &str) -> String {
    format!("CASE WHEN {column} IS NULL THEN NULL ELSE {value} END")
}

impl MetadataColumn {
    /// Clé de la valeur d'un média, en SQL (sur `media`). Le mot-clé n'en a
    /// pas : un média peut en porter plusieurs (voir `keyword_values`).
    pub(super) fn key_sql(self) -> String {
        match self {
            Self::Date => "nullif(substr(media.capture_time, 1, 10), '')".to_owned(),
            Self::Camera => format!(
                "coalesce({}, {})",
                text("media.camera_model"),
                text("media.camera_make")
            ),
            Self::Lens => text("media.lens"),
            Self::ColorLabel => "media.color_label".to_owned(),
            Self::Keyword => "NULL".to_owned(),
            Self::Country => text("media.country"),
            Self::Region => text("media.region"),
            Self::City => text("media.city"),
            Self::Location => text("media.location"),
            Self::FileType => text("media.format"),
            Self::Iso => when_known("media.iso", "CAST(media.iso AS TEXT)"),
            Self::Aperture => when_known("media.aperture", "printf('%.1f', media.aperture)"),
            Self::ShutterSpeed => {
                when_known("media.exposure_time", "printf('%.6g', media.exposure_time)")
            }
            Self::FocalLength => when_known(
                "media.focal_length",
                "CAST(CAST(round(media.focal_length) AS INTEGER) AS TEXT)",
            ),
            Self::Flash => {
                "CASE media.flash_fired WHEN 1 THEN 'fired' WHEN 0 THEN 'notFired' END".to_owned()
            }
            Self::Creator => text("media.creator"),
            Self::Dimensions => "media.width || '×' || media.height".to_owned(),
            // Une orientation EXIF de 5 à 8, ou une rotation d'un quart de
            // tour, échange largeur et hauteur ; les deux s'annulent.
            Self::Orientation => "CASE
                WHEN media.width IS NULL OR media.height IS NULL THEN NULL
                WHEN media.width = media.height THEN 'square'
                WHEN (media.width > media.height)
                    = ((coalesce(media.orientation, 1) >= 5) = (media.rotation IN (90, 270)))
                    THEN 'landscape'
                ELSE 'portrait' END"
                .to_owned(),
        }
    }

    /// Ordre des valeurs dans la colonne (« Inconnu » à la fin) : les dates
    /// les plus récentes d'abord, les nombres dans l'ordre croissant, les
    /// textes dans l'ordre alphabétique.
    pub(super) fn order_sql(self) -> &'static str {
        match self {
            Self::Date => "value IS NULL, value DESC",
            Self::Iso => "value IS NULL, min(media.iso)",
            Self::Aperture => "value IS NULL, min(media.aperture)",
            Self::ShutterSpeed => "value IS NULL, min(media.exposure_time)",
            Self::FocalLength => "value IS NULL, min(media.focal_length)",
            Self::Dimensions => "value IS NULL, min(media.width * media.height)",
            Self::ColorLabel => {
                "value IS NULL, CASE value WHEN 'red' THEN 1 WHEN 'yellow' THEN 2 \
                 WHEN 'green' THEN 3 WHEN 'blue' THEN 4 ELSE 5 END"
            }
            _ => "value IS NULL, value COLLATE NOCASE",
        }
    }

    /// Condition « le média a cette valeur », avec `parameter` pour la clé.
    fn value_condition(self, value: Option<&str>, parameter: &str) -> String {
        match (self, value) {
            (Self::Keyword, Some(_)) => format!(
                "media.id IN (SELECT media_id FROM media_keywords WHERE keyword_id = {parameter})"
            ),
            (Self::Keyword, None) => {
                "NOT EXISTS (SELECT 1 FROM media_keywords WHERE media_id = media.id)".to_owned()
            }
            // Une année ou un mois retient tous ses jours.
            (Self::Date, Some(_)) => {
                format!("substr(media.capture_time, 1, length({parameter})) = {parameter}")
            }
            (_, Some(_)) => format!("{} = {parameter}", self.key_sql()),
            (_, None) => format!("{} IS NULL", self.key_sql()),
        }
    }
}

/// Condition SQL des choix des colonnes, et valeurs de ses paramètres
/// (`:meta_0`, `:meta_1`…) : dans une colonne, les valeurs s'additionnent ;
/// entre colonnes, elles se cumulent. Une colonne sans choix ne retient
/// rien de plus.
pub(super) fn choices_condition(
    choices: &[ColumnChoice],
) -> Option<(String, Vec<(String, String)>)> {
    let mut parts = Vec::new();
    let mut params = Vec::new();
    for choice in choices.iter().filter(|choice| !choice.values.is_empty()) {
        let alternatives: Vec<String> = choice
            .values
            .iter()
            .map(|value| {
                let parameter = format!(":meta_{}", params.len());
                let condition = choice.column.value_condition(value.as_deref(), &parameter);
                if let Some(value) = value {
                    params.push((parameter, value.clone()));
                }
                condition
            })
            .collect();
        parts.push(format!("({})", alternatives.join(" OR ")));
    }
    (!parts.is_empty()).then(|| (parts.join(" AND "), params))
}

/// Valeurs d'une colonne parmi les médias de la demande (source et filtres,
/// dont les choix des colonnes à sa gauche), avec leur nombre de médias.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn column_values(
    connection: &Connection,
    query: &LibraryQuery,
    column: MetadataColumn,
) -> Result<Vec<ColumnValue>, CatalogError> {
    if column == MetadataColumn::Keyword {
        return keyword_values(connection, query);
    }
    let sql = query.sql(connection)?;
    let rows = connection
        .prepare_cached(&format!(
            "SELECT {} AS value, count(*) FROM media WHERE {} GROUP BY value ORDER BY {}",
            column.key_sql(),
            sql.condition,
            column.order_sql()
        ))?
        .query_map(sql.params(&[]).as_slice(), |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(value, count)| {
            Ok(ColumnValue {
                value,
                label: None,
                detail: None,
                count: size_from_sql(count)?,
            })
        })
        .collect()
}

/// Mots-clés posés sur les médias de la demande, avec leur parent, par ordre
/// alphabétique ; puis « Inconnu » : les médias sans mot-clé.
fn keyword_values(
    connection: &Connection,
    query: &LibraryQuery,
) -> Result<Vec<ColumnValue>, CatalogError> {
    let sql = query.sql(connection)?;
    let rows = connection
        .prepare_cached(&format!(
            "SELECT keywords.id, keywords.name, parent.name, count(*)
             FROM media_keywords
             JOIN media ON media.id = media_keywords.media_id
             JOIN keywords ON keywords.id = media_keywords.keyword_id
             LEFT JOIN keywords AS parent ON parent.id = keywords.parent_id
             WHERE {}
             GROUP BY keywords.id ORDER BY keywords.search_key, parent.search_key",
            sql.condition
        ))?
        .query_map(sql.params(&[]).as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut values = rows
        .into_iter()
        .map(|(id, name, parent, count)| {
            Ok(ColumnValue {
                value: Some(id.to_string()),
                label: Some(name),
                detail: parent,
                count: size_from_sql(count)?,
            })
        })
        .collect::<Result<Vec<_>, CatalogError>>()?;
    let untagged: i64 = connection
        .prepare_cached(&format!(
            "SELECT count(*) FROM media WHERE {} AND NOT EXISTS
                (SELECT 1 FROM media_keywords WHERE media_id = media.id)",
            sql.condition
        ))?
        .query_row(sql.params(&[]).as_slice(), |row| row.get(0))?;
    if untagged > 0 {
        values.push(ColumnValue {
            value: None,
            label: None,
            detail: None,
            count: size_from_sql(untagged)?,
        });
    }
    Ok(values)
}
