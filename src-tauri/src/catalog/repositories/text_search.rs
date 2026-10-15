//! Filtre Texte de la grille : les médias d'une source dont le
//! texte est retenu par la règle.
//!
//! La comparaison se fait en Rust, pas en SQL : remonter les parents des
//! mots-clés de chaque photo dans la requête coûtait près d'une demi-seconde
//! pour 50 000 photos ; ici, le chemin de chaque mot-clé n'est calculé
//! qu'une fois.

use std::collections::HashMap;

use rusqlite::{Connection, ToSql};

use crate::catalog::CatalogError;
use crate::domain::{TextField, TextRule, TextTerms, search_key};

/// Nombre maximal de niveaux remontés depuis un mot-clé : borne le parcours
/// même en cas de boucle dans une base abîmée.
const MAX_KEYWORD_DEPTH: usize = 100;

/// Filtre Texte : la saisie, le champ cherché et la règle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearch {
    /// Champ cherché.
    pub field: TextField,
    /// Règle.
    pub rule: TextRule,
    /// Termes saisis, séparés par des virgules ou des espaces.
    pub text: String,
}

/// Champs d'un média lus pour la recherche.
struct MediaText {
    id: i64,
    file_name: String,
    title: Option<String>,
    caption: Option<String>,
    creator: Option<String>,
    copyright: Option<String>,
    place: [Option<String>; 4],
}

impl TextSearch {
    /// Vrai si la saisie contient au moins un terme.
    pub(super) fn has_terms(&self) -> bool {
        !TextTerms::parse(&self.text).is_empty()
    }

    /// Identifiants des médias retenus parmi ceux de `source_condition`
    /// (condition SQL sur `media`, avec `:source` pour valeur `source`).
    pub(super) fn matching_ids(
        &self,
        connection: &Connection,
        source_condition: &str,
        source: Option<i64>,
    ) -> Result<Vec<i64>, CatalogError> {
        let terms = TextTerms::parse(&self.text);
        let keywords = if matches!(self.field, TextField::All | TextField::Keywords) {
            keyword_text_by_media(connection, source_condition, source)?
        } else {
            HashMap::new()
        };
        let mut statement = connection.prepare_cached(&format!(
            "SELECT media.id, media.file_name, media.title, media.caption, media.creator,
                media.copyright, media.country, media.region, media.city, media.location
             FROM media WHERE {source_condition}"
        ))?;
        let rows = statement.query_map(source_params(source.as_ref()).as_slice(), |row| {
            Ok(MediaText {
                id: row.get(0)?,
                file_name: row.get(1)?,
                title: row.get(2)?,
                caption: row.get(3)?,
                creator: row.get(4)?,
                copyright: row.get(5)?,
                place: [row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?],
            })
        })?;
        let mut ids = Vec::new();
        for media in rows {
            let media = media?;
            let keyword_text = keywords.get(&media.id).map(String::as_str);
            if terms.matches_key(self.rule, &self.text_of(&media, keyword_text)) {
                ids.push(media.id);
            }
        }
        Ok(ids)
    }

    /// Texte du champ cherché d'un média, sans majuscules ni accents, un
    /// champ par ligne ; `keywords` l'est déjà.
    fn text_of(&self, media: &MediaText, keywords: Option<&str>) -> String {
        let place = media.place.iter().map(Option::as_deref);
        let fields: Vec<Option<&str>> = match self.field {
            TextField::All => [
                Some(media.file_name.as_str()),
                media.title.as_deref(),
                media.caption.as_deref(),
                media.creator.as_deref(),
                media.copyright.as_deref(),
            ]
            .into_iter()
            .chain(place)
            .collect(),
            TextField::FileName => vec![Some(media.file_name.as_str())],
            TextField::Title => vec![media.title.as_deref()],
            TextField::Caption => vec![media.caption.as_deref()],
            TextField::Keywords => Vec::new(),
            TextField::Place => place.collect(),
        };
        let mut text = search_key(&fields.into_iter().flatten().collect::<Vec<_>>().join("\n"));
        if let Some(keywords) = keywords {
            text.push('\n');
            text.push_str(keywords);
        }
        text
    }
}

/// Paramètres d'une requête qui n'a que `:source`, s'il y en a un.
fn source_params(source: Option<&i64>) -> Vec<(&str, &dyn ToSql)> {
    source
        .map(|value| vec![(":source", value as &dyn ToSql)])
        .unwrap_or_default()
}

/// Mots-clés posés sur chaque média de la source, avec tous leurs parents
/// (« lieux⏎france⏎paris »), sans majuscules ni accents : « France »
/// trouve ainsi une photo de Paris.
fn keyword_text_by_media(
    connection: &Connection,
    source_condition: &str,
    source: Option<i64>,
) -> Result<HashMap<i64, String>, CatalogError> {
    let tree: HashMap<i64, (Option<i64>, String)> = connection
        .prepare_cached("SELECT id, parent_id, name FROM keywords")?
        .query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?
        .collect::<Result<_, _>>()?;
    let mut paths: HashMap<i64, String> = HashMap::new();
    let mut by_media: HashMap<i64, String> = HashMap::new();
    let mut statement = connection.prepare_cached(&format!(
        "SELECT media_keywords.media_id, media_keywords.keyword_id
         FROM media_keywords JOIN media ON media.id = media_keywords.media_id
         WHERE {source_condition}"
    ))?;
    let rows = statement.query_map(source_params(source.as_ref()).as_slice(), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (media_id, keyword_id) = row?;
        let path = paths
            .entry(keyword_id)
            .or_insert_with(|| keyword_path(&tree, keyword_id));
        let text = by_media.entry(media_id).or_default();
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(path);
    }
    Ok(by_media)
}

/// Le mot-clé et ses parents, un nom par ligne, du mot-clé à la racine.
fn keyword_path(tree: &HashMap<i64, (Option<i64>, String)>, id: i64) -> String {
    let mut names = Vec::new();
    let mut current = Some(id);
    while let Some(id) = current {
        let Some((parent, name)) = tree.get(&id) else {
            break;
        };
        names.push(name.as_str());
        if names.len() >= MAX_KEYWORD_DEPTH {
            break;
        }
        current = *parent;
    }
    search_key(&names.join("\n"))
}
