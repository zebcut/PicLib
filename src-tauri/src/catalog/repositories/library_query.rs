//! Ce que la grille affiche : une source, filtrée (filtres Texte et Attribut) et triée, traduite en SQL.

use std::rc::Rc;

use rusqlite::types::Value;
use rusqlite::vtab::array::Array;
use rusqlite::{Connection, ToSql};

use super::map::GeoArea;
use super::metadata_columns::{ColumnChoice, choices_condition};
use super::smart_sql::{self, SmartValue};
use super::stacks;
use super::text_search::TextSearch;
use crate::catalog::CatalogError;
use crate::catalog::sql::{color_label_to_text, media_format_to_text};
use crate::domain::{CollectionId, ColorLabel, FolderId, ImportId, KeywordId, MediaFormat};

/// Ensemble d'éléments affichés dans la grille.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibrarySource {
    /// Tout le catalogue.
    #[default]
    All,
    /// Les éléments ajoutés par un import donné.
    Import(ImportId),
    /// Les éléments qui portent ce mot-clé ou un de ses descendants
    ///.
    Keyword(KeywordId),
    /// Les éléments d'un dossier et de ses sous-dossiers.
    Folder(FolderId),
    /// Les photos d'une collection, ou de toutes les collections d'un
    /// dossier de collections, chacune une fois.
    Collection(CollectionId),
    /// La collection rapide.
    QuickCollection,
    /// Les photos dont le fichier est introuvable.
    Missing,
}

/// Photos de la collection `:source`, ou des collections du dossier
/// `:source` ; `UNION` arrête le parcours même en cas de boucle dans une
/// base abîmée.
const COLLECTION_FILTER: &str = "media.id IN (
    SELECT collection_media.media_id FROM collection_media
    WHERE collection_media.collection_id IN (
        WITH RECURSIVE branch(id) AS (
            SELECT :source
            UNION
            SELECT collections.id FROM collections
            JOIN branch ON collections.parent_id = branch.id
        )
        SELECT id FROM branch))";

/// Médias qui portent le mot-clé `:source` ou un de ses descendants. La
/// branche est parcourue par l'index `keywords_unique_name` (d'où le
/// `coalesce`, qui reprend son expression) ; `UNION`, qui écarte les
/// doublons, arrête le parcours même en cas de boucle dans une base abîmée.
const KEYWORD_FILTER: &str = "media.id IN (
    SELECT media_keywords.media_id FROM media_keywords
    WHERE media_keywords.keyword_id IN (
        WITH RECURSIVE branch(id) AS (
            SELECT :source
            UNION
            SELECT keywords.id FROM keywords
            JOIN branch ON coalesce(keywords.parent_id, 0) = branch.id
        )
        SELECT id FROM branch))";

/// Médias du dossier `:source` ou d'un de ses sous-dossiers, parcourus par
/// l'index `folders_by_parent` ; `UNION` arrête le parcours même en cas de
/// boucle dans une base abîmée.
const FOLDER_FILTER: &str = "media.folder_id IN (
    WITH RECURSIVE branch(id) AS (
        SELECT :source
        UNION
        SELECT folders.id FROM folders JOIN branch ON folders.parent_id = branch.id
    )
    SELECT id FROM branch)";

impl LibrarySource {
    /// Condition SQL qui retient les médias de la source, et la valeur de
    /// son paramètre `:source` s'il en a un.
    fn filter(self) -> (&'static str, Option<i64>) {
        match self {
            Self::All => ("1", None),
            Self::Import(import) => ("media.import_id = :source", Some(import.get())),
            Self::Keyword(keyword) => (KEYWORD_FILTER, Some(keyword.get())),
            Self::Folder(folder) => (FOLDER_FILTER, Some(folder.get())),
            Self::Collection(collection) => (COLLECTION_FILTER, Some(collection.get())),
            Self::QuickCollection => ("media.id IN (SELECT media_id FROM quick_collection)", None),
            Self::Missing => ("media.file_missing = 1", None),
        }
    }
}

/// Requête SQL d'une demande : condition, ordre et valeurs de ses
/// paramètres.
pub(super) struct QuerySql {
    /// Condition sur `media` (source et filtres).
    pub(super) condition: String,
    /// Clause `ORDER BY`.
    pub(super) order: String,
    /// Valeur de `:source`, si la source en a une.
    source: Option<i64>,
    /// Valeur de `:text_ids` (médias retenus par le filtre Texte), s'il agit.
    text_ids: Option<Array>,
    /// Valeurs de `:meta_0`, `:meta_1`… (choix du filtre Métadonnées).
    metadata: Vec<(String, String)>,
    /// Valeurs de `:smart_0`, `:smart_1`… (règles des collections
    /// dynamiques de la source).
    smart: Vec<(String, SmartValue)>,
}

impl QuerySql {
    /// Paramètres nommés : `others`, plus ceux de la condition (SQLite refuse
    /// un paramètre absent de la requête).
    pub(super) fn params<'a>(
        &'a self,
        others: &[(&'a str, &'a dyn ToSql)],
    ) -> Vec<(&'a str, &'a dyn ToSql)> {
        let mut params = others.to_vec();
        if let Some(value) = &self.source {
            params.push((":source", value));
        }
        if let Some(ids) = &self.text_ids {
            params.push((":text_ids", ids));
        }
        for (name, value) in &self.metadata {
            params.push((name.as_str(), value));
        }
        for (name, value) in &self.smart {
            params.push((name.as_str(), value));
        }
        params
    }
}

/// Marqueur retenu par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagChoice {
    /// Retenue.
    Picked,
    /// Sans marqueur.
    Unflagged,
    /// Rejetée.
    Rejected,
}

/// Comparaison de la note avec un nombre d'étoiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingComparison {
    /// Exactement ce nombre d'étoiles.
    Equal,
    /// Au moins ce nombre d'étoiles.
    AtLeast,
    /// Au plus ce nombre d'étoiles.
    AtMost,
}

/// Règle de note du filtre Attribut (« ≥ 3 étoiles »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatingRule {
    /// Comparaison.
    pub comparison: RatingComparison,
    /// Nombre d'étoiles, de 0 à 5.
    pub stars: u8,
}

/// Étiquette de couleur retenue par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Cette couleur.
    Label(ColorLabel),
    /// Sans couleur.
    Unlabeled,
}

/// Type de fichier retenu par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindChoice {
    /// Photo (RAW compris).
    Photo,
    /// Vidéo.
    Video,
    /// Photo RAW.
    Raw,
    /// Photo d'une pile.
    Stack,
}

/// Filtre Attribut. Dans chaque critère, les choix cochés se
/// cumulent (« retenues ou sans marqueur ») ; les critères s'ajoutent les
/// uns aux autres (« retenues et ≥ 3 étoiles »). Un critère vide ne retient
/// rien de plus : le filtre par défaut laisse tout passer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    /// Marqueurs retenus.
    pub flags: Vec<FlagChoice>,
    /// Règle de note.
    pub rating: Option<RatingRule>,
    /// Étiquettes de couleur retenues.
    pub colors: Vec<ColorChoice>,
    /// Types de fichiers retenus.
    pub kinds: Vec<KindChoice>,
    /// Filtre Texte, qui s'ajoute aux critères Attribut.
    pub text: Option<TextSearch>,
    /// Choix du filtre Métadonnées, colonne par colonne.
    pub metadata: Vec<ColumnChoice>,
    /// Zone de la carte : seules les photos qui y sont situées.
    pub area: Option<GeoArea>,
}

impl LibraryFilter {
    /// Filtre Texte qui agit : avec au moins un terme.
    fn active_text(&self) -> Option<&TextSearch> {
        self.text.as_ref().filter(|text| text.has_terms())
    }

    /// Condition SQL du filtre, et valeurs de ses paramètres `:meta_…`. Elle
    /// n'est faite que de valeurs connues (énumérations, nombre d'étoiles) :
    /// le filtre Texte passe par la liste des médias qu'il retient
    /// (`:text_ids`), les valeurs des Métadonnées par des paramètres.
    fn condition(&self) -> (String, Vec<(String, String)>) {
        let mut parts = Vec::new();
        if self.active_text().is_some() {
            parts.push("media.id IN rarray(:text_ids)".to_owned());
        }
        let mut params = Vec::new();
        if let Some((condition, values)) = choices_condition(&self.metadata) {
            parts.push(condition);
            params = values;
        }
        if let Some(area) = self.area {
            parts.push(format!(
                "media.gps_latitude IS NOT NULL AND media.gps_longitude IS NOT NULL AND {}",
                area.condition()
            ));
        }
        if !self.flags.is_empty() {
            parts.push(any_of(self.flags.iter().map(|flag| {
                match flag {
                    FlagChoice::Picked => "media.flag = 'picked'",
                    FlagChoice::Unflagged => "media.flag IS NULL",
                    FlagChoice::Rejected => "media.flag = 'rejected'",
                }
                .to_owned()
            })));
        }
        if let Some(rule) = self.rating {
            let operator = match rule.comparison {
                RatingComparison::Equal => "=",
                RatingComparison::AtLeast => ">=",
                RatingComparison::AtMost => "<=",
            };
            parts.push(format!("media.rating {operator} {}", rule.stars));
        }
        if !self.colors.is_empty() {
            parts.push(any_of(self.colors.iter().map(|color| match color {
                ColorChoice::Label(label) => {
                    format!("media.color_label = '{}'", color_label_to_text(*label))
                }
                ColorChoice::Unlabeled => "media.color_label IS NULL".to_owned(),
            })));
        }
        if !self.kinds.is_empty() {
            parts.push(any_of(self.kinds.iter().map(|kind| match kind {
                KindChoice::Photo => "media.kind = 'photo'".to_owned(),
                KindChoice::Video => "media.kind = 'video'".to_owned(),
                KindChoice::Stack => stacks::IN_STACK.to_owned(),
                KindChoice::Raw => {
                    let formats: Vec<String> = MediaFormat::ALL
                        .into_iter()
                        .filter(|format| format.is_raw())
                        .map(|format| format!("'{}'", media_format_to_text(format)))
                        .collect();
                    format!("media.format IN ({})", formats.join(", "))
                }
            })));
        }
        let condition = if parts.is_empty() {
            "1".to_owned()
        } else {
            parts.join(" AND ")
        };
        (condition, params)
    }
}

/// Une condition parmi plusieurs, entre parenthèses.
fn any_of(conditions: impl Iterator<Item = String>) -> String {
    format!("({})", conditions.collect::<Vec<_>>().join(" OR "))
}

/// Critère de tri de la grille.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Date de prise de vue.
    #[default]
    CaptureTime,
    /// Date d'import.
    ImportTime,
    /// Nom de fichier, sans tenir compte de la casse.
    FileName,
    /// Note.
    Rating,
    /// Marqueur : retenue, sans marqueur, rejetée.
    Flag,
    /// Étiquette de couleur, dans l'ordre des touches 6 à 9, puis violet,
    /// puis sans couleur.
    ColorLabel,
    /// Photo ou vidéo, puis format.
    FileType,
}

/// Tri de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibrarySort {
    /// Critère.
    pub key: SortKey,
    /// Ordre décroissant : les plus récentes, les mieux notées, la fin de
    /// l'alphabet d'abord.
    pub descending: bool,
}

impl Default for LibrarySort {
    /// Les plus récentes d'abord.
    fn default() -> Self {
        Self {
            key: SortKey::CaptureTime,
            descending: true,
        }
    }
}

impl LibrarySort {
    /// Clause `ORDER BY`. À valeur égale, la date de prise de vue puis
    /// l'ordre d'ajout départagent, dans le même sens : l'ordre est toujours
    /// identique d'une page à l'autre.
    fn order_by(self) -> String {
        let direction = if self.descending { "DESC" } else { "ASC" };
        let columns: &[&str] = match self.key {
            SortKey::CaptureTime => &[],
            SortKey::ImportTime => &["media.import_id"],
            SortKey::FileName => &["media.file_name COLLATE NOCASE"],
            SortKey::Rating => &["media.rating"],
            SortKey::Flag => {
                &["CASE media.flag WHEN 'picked' THEN 2 WHEN 'rejected' THEN 0 ELSE 1 END"]
            }
            SortKey::ColorLabel => &["CASE media.color_label WHEN 'red' THEN 1 \
                WHEN 'yellow' THEN 2 WHEN 'green' THEN 3 WHEN 'blue' THEN 4 \
                WHEN 'purple' THEN 5 ELSE 6 END"],
            SortKey::FileType => &["media.kind", "media.format"],
        };
        let terms: Vec<String> = columns
            .iter()
            .chain(&["media.capture_time", "media.id"])
            .map(|column| format!("{column} {direction}"))
            .collect();
        format!("ORDER BY {}", terms.join(", "))
    }
}

/// Éléments à afficher : une source, filtrée et triée.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryQuery {
    /// Source.
    pub source: LibrarySource,
    /// Filtre.
    pub filter: LibraryFilter,
    /// Tri.
    pub sort: LibrarySort,
}

impl From<LibrarySource> for LibraryQuery {
    /// Toute la source, les plus récentes d'abord.
    fn from(source: LibrarySource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

impl LibraryQuery {
    /// Requête SQL de la demande ; le filtre Texte, s'il agit, est évalué
    /// ici sur les médias de la source.
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub(super) fn sql(&self, connection: &Connection) -> Result<QuerySql, CatalogError> {
        self.sql_with(connection, false)
    }

    /// Requête SQL de ce que la grille affiche : comme [`Self::sql`], sans
    /// les photos recouvertes par une pile repliée.
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub(super) fn grid_sql(&self, connection: &Connection) -> Result<QuerySql, CatalogError> {
        self.sql_with(connection, stacks::has_collapsed(connection)?)
    }

    fn sql_with(
        &self,
        connection: &Connection,
        hide_stacked: bool,
    ) -> Result<QuerySql, CatalogError> {
        let (static_source, value) = self.source.filter();
        let (source, smart) = smart_sql::source_condition(connection, self.source, static_source)?;
        let text_ids = match self.filter.active_text() {
            Some(text) => {
                // Avec des règles, la source a ses propres paramètres : le
                // texte est cherché dans tout le catalogue, la condition
                // finale ne gardant que la source.
                let (scope, scope_value) = if smart.is_empty() {
                    (source.as_str(), value)
                } else {
                    ("1", None)
                };
                let ids = text.matching_ids(connection, scope, scope_value)?;
                Some(Rc::new(ids.into_iter().map(Value::from).collect()))
            }
            None => None,
        };
        let (condition, metadata) = self.filter.condition();
        let mut condition = format!("({source}) AND {condition}");
        if hide_stacked {
            // Les paramètres nommés reparaissent dans la sous-requête : SQLite
            // leur donne la même valeur.
            condition = format!("{condition} AND {}", stacks::hidden_condition(&condition));
        }
        Ok(QuerySql {
            condition,
            order: self.sort.order_by(),
            source: value,
            text_ids,
            metadata,
            smart,
        })
    }
}
