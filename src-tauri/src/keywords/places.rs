//! Mots-clés de lieu : les champs Pays, Région,
//! Ville et Lieu précis d'un média deviennent un mot-clé rangé sous
//! « Lieux » (`Lieux > France > Bretagne > Quimper > Cathédrale`).
//!
//! Les champs font foi : le « mot-clé de lieu » d'un média est celui qui
//! correspond exactement à ses champs. Quand ils changent, l'ancien est
//! retiré du média (il reste dans la liste) et le nouveau posé. Un niveau
//! manquant devient « Sans pays », « Sans région » ou « Sans ville » ; un
//! niveau vide en fin de chemin est omis. « Lieux » et ces niveaux de
//! rangement ne sont pas exportés.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::catalog::repositories::{keyword_changes, keywords, metadata};
use crate::domain::{KeywordId, MediaId, MetadataField};

/// Nom donné à la catégorie à sa création (l'utilisateur peut la renommer).
pub const ROOT_NAME: &str = "Lieux";

/// Champs de lieu, dans l'ordre de l'arbre.
pub const PLACE_FIELDS: [MetadataField; 4] = [
    MetadataField::Country,
    MetadataField::Region,
    MetadataField::City,
    MetadataField::Location,
];

/// Noms des niveaux manquants (le lieu précis, dernier niveau, n'en a pas
/// besoin : un niveau vide en fin de chemin est omis).
const MISSING_LEVELS: [&str; 3] = ["Sans pays", "Sans région", "Sans ville"];

/// Champs de lieu d'un média ; une valeur vide compte comme absente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PlaceFields([Option<String>; 4]);

impl PlaceFields {
    /// Champs donnés dans l'ordre Pays, Région, Ville, Lieu précis.
    #[must_use]
    pub fn new(values: [Option<&str>; 4]) -> Self {
        Self(values.map(clean))
    }

    /// Mêmes champs, avec `field` à `value` (sans effet pour un autre
    /// champ que les quatre de lieu).
    #[must_use]
    pub fn with(&self, field: MetadataField, value: Option<&str>) -> Self {
        let mut fields = self.clone();
        if let Some(index) = PLACE_FIELDS.iter().position(|place| *place == field) {
            fields.0[index] = clean(value);
        }
        fields
    }

    /// Chemin du mot-clé de lieu sous « Lieux » : chaque nom, et vrai pour
    /// un niveau manquant (« Sans région »). Vide si aucun champ n'est
    /// rempli.
    #[must_use]
    pub fn path(&self) -> PlacePath {
        let Some(last) = self.0.iter().rposition(Option::is_some) else {
            return Vec::new();
        };
        self.0[..=last]
            .iter()
            .enumerate()
            .map(|(level, value)| match value {
                Some(name) => (name.clone(), false),
                None => (MISSING_LEVELS[level.min(2)].to_owned(), true),
            })
            .collect()
    }
}

/// Valeur d'un champ, sans espaces autour ; vide : absente.
fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Chemin d'un mot-clé de lieu sous « Lieux » : chaque nom, et vrai pour un
/// niveau manquant.
type PlacePath = Vec<(String, bool)>;

/// Champs de lieu des médias `ids` (les identifiants inconnus sont absents).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn read(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<HashMap<MediaId, PlaceFields>, CatalogError> {
    let mut fields: HashMap<MediaId, PlaceFields> = HashMap::new();
    for (index, field) in PLACE_FIELDS.into_iter().enumerate() {
        for (id, value) in metadata::read(connection, field, ids)? {
            fields.entry(id).or_default().0[index] = clean(value.as_deref());
        }
    }
    Ok(fields)
}

/// Liens entre médias et mots-clés de lieu à retirer et à poser.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceLinks {
    /// Anciens mots-clés de lieu, à retirer.
    pub unlink: Vec<(MediaId, KeywordId)>,
    /// Nouveaux mots-clés de lieu, à poser.
    pub link: Vec<(MediaId, KeywordId)>,
}

impl PlaceLinks {
    /// Vrai s'il n'y a rien à changer.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unlink.is_empty() && self.link.is_empty()
    }

    /// Retire les anciens liens et pose les nouveaux.
    ///
    /// # Errors
    ///
    /// Échoue si la base ne peut pas être écrite.
    pub fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        keywords::unlink(connection, &self.unlink)?;
        keywords::link(connection, &self.link)
    }

    /// Défait [`Self::apply`].
    ///
    /// # Errors
    ///
    /// Échoue si la base ne peut pas être écrite.
    pub fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        keywords::unlink(connection, &self.link)?;
        keywords::link(connection, &self.unlink)
    }
}

/// Liens à changer quand les champs de lieu passent de `before` à `after`
/// (médias absents de `after` : inchangés). Les mots-clés du nouveau lieu
/// sont créés s'il le faut (ils restent si la modification est annulée,
/// comme ceux d'une saisie).
///
/// # Errors
///
/// Échoue si la base est illisible ou ne peut pas être écrite.
pub fn changes<S: BuildHasher>(
    connection: &Connection,
    before: &HashMap<MediaId, PlaceFields, S>,
    after: &HashMap<MediaId, PlaceFields, S>,
) -> Result<PlaceLinks, CatalogError> {
    let changed: Vec<(MediaId, PlacePath, PlacePath)> = after
        .iter()
        .filter_map(|(id, new)| {
            let old = before.get(id).map(PlaceFields::path).unwrap_or_default();
            let new = new.path();
            (old != new).then_some((*id, old, new))
        })
        .collect();
    if changed.is_empty() {
        return Ok(PlaceLinks::default());
    }
    let ids: Vec<MediaId> = changed.iter().map(|(id, _, _)| *id).collect();
    let linked: HashSet<(MediaId, KeywordId)> =
        keywords::links(connection, &ids)?.into_iter().collect();
    let mut leaves = Leaves::new(connection)?;
    let mut links = PlaceLinks::default();
    for (id, old, new) in changed {
        let old_leaf = leaves.find(connection, &old)?;
        let new_leaf = leaves.ensure(connection, &new)?;
        if let Some(old_leaf) = old_leaf
            && Some(old_leaf) != new_leaf
            && linked.contains(&(id, old_leaf))
        {
            links.unlink.push((id, old_leaf));
        }
        if let Some(new_leaf) = new_leaf
            && !linked.contains(&(id, new_leaf))
        {
            links.link.push((id, new_leaf));
        }
    }
    Ok(links)
}

/// Pose sur les médias `ids` le mot-clé de lieu de leurs champs actuels
/// (médias qui viennent d'être ajoutés, rattrapage), sans rien retirer.
///
/// # Errors
///
/// Échoue si la base est illisible ou ne peut pas être écrite.
pub fn link_current(connection: &Connection, ids: &[MediaId]) -> Result<usize, CatalogError> {
    let current = read(connection, ids)?;
    let links = changes(connection, &HashMap::default(), &current)?;
    links.apply(connection)?;
    Ok(links.link.len())
}

/// Rattrapage, une seule fois par catalogue : pose sur tous les médias déjà
/// au catalogue le mot-clé de lieu de leurs champs. Renvoie le nombre de
/// mots-clés posés. À appeler dans une transaction.
///
/// # Errors
///
/// Échoue si la base est illisible ou ne peut pas être écrite.
pub fn backfill(connection: &Connection) -> Result<usize, CatalogError> {
    let done: bool = connection
        .prepare_cached("SELECT backfilled FROM place_keywords WHERE id = 1")?
        .query_row([], |row| row.get(0))
        .optional()?
        .unwrap_or(true);
    if done {
        return Ok(0);
    }
    let ids: Vec<MediaId> = connection
        .prepare_cached(
            "SELECT id FROM media
             WHERE coalesce(country, region, city, location) IS NOT NULL",
        )?
        .query_map([], |row| Ok(MediaId::new(row.get(0)?)))?
        .collect::<Result<_, _>>()?;
    let linked = link_current(connection, &ids)?;
    connection.execute("UPDATE place_keywords SET backfilled = 1 WHERE id = 1", [])?;
    if linked > 0 {
        tracing::info!(linked, "place keywords added to existing media");
    }
    Ok(linked)
}

/// Champs vides des médias que remplissent les mots-clés de « Lieux » qu'on
/// leur pose (poser un lieu à la main ne remplit que les champs vides ; les niveaux manquants, « Sans région », ne remplissent rien).
/// Renvoie, par champ et valeur, les médias à remplir.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn fields_to_fill(
    connection: &Connection,
    links: &[(MediaId, KeywordId)],
) -> Result<Vec<(MetadataField, String, Vec<MediaId>)>, CatalogError> {
    let Some(root) = existing_root(connection)? else {
        return Ok(Vec::new());
    };
    let mut by_keyword: HashMap<KeywordId, PlaceFields> = HashMap::new();
    for &(_, keyword) in links {
        if let std::collections::hash_map::Entry::Vacant(entry) = by_keyword.entry(keyword) {
            entry.insert(keyword_fields(connection, root, keyword)?);
        }
    }
    let ids: Vec<MediaId> = links.iter().map(|&(media, _)| media).collect();
    let mut current = read(connection, &ids)?;
    let mut fills: HashMap<(MetadataField, String), Vec<MediaId>> = HashMap::new();
    for &(media, keyword) in links {
        let (Some(given), Some(fields)) = (by_keyword.get(&keyword), current.get_mut(&media))
        else {
            continue;
        };
        for (index, field) in PLACE_FIELDS.into_iter().enumerate() {
            if let (None, Some(value)) = (&fields.0[index], &given.0[index]) {
                fills.entry((field, value.clone())).or_default().push(media);
                // Un second lieu posé en même temps ne remplace pas le
                // premier.
                fields.0[index] = Some(value.clone());
            }
        }
    }
    Ok(fills
        .into_iter()
        .map(|((field, value), ids)| (field, value, ids))
        .collect())
}

/// Champs que donne un mot-clé : son chemin sous `root` (vide s'il n'est
/// pas rangé sous « Lieux »), niveaux manquants exclus.
fn keyword_fields(
    connection: &Connection,
    root: KeywordId,
    keyword: KeywordId,
) -> Result<PlaceFields, CatalogError> {
    let chain: Vec<(i64, String)> = connection
        .prepare_cached(
            "WITH RECURSIVE up(id, parent_id, name, depth) AS (
                 SELECT id, parent_id, name, 0 FROM keywords WHERE id = ?1
                 UNION ALL
                 SELECT keywords.id, keywords.parent_id, keywords.name, up.depth + 1
                 FROM keywords JOIN up ON keywords.id = up.parent_id
                 WHERE up.depth < 64
             )
             SELECT id, name FROM up ORDER BY depth DESC",
        )?
        .query_map([keyword.get()], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut fields = PlaceFields::default();
    if chain.first().map(|(id, _)| *id) != Some(root.get()) {
        return Ok(fields);
    }
    for (level, (_, name)) in chain.iter().skip(1).take(PLACE_FIELDS.len()).enumerate() {
        if MISSING_LEVELS.get(level) != Some(&name.as_str()) {
            fields.0[level] = clean(Some(name));
        }
    }
    Ok(fields)
}

/// Mot-clé « Lieux » et mots-clés de lieu déjà trouvés ou créés, le temps
/// d'une opération : des milliers de médias partagent quelques lieux.
struct Leaves {
    root: Option<KeywordId>,
    found: HashMap<PlacePath, Option<KeywordId>>,
}

impl Leaves {
    fn new(connection: &Connection) -> Result<Self, CatalogError> {
        Ok(Self {
            root: existing_root(connection)?,
            found: HashMap::new(),
        })
    }

    /// Mot-clé du chemin `path`, s'il existe déjà.
    fn find(
        &mut self,
        connection: &Connection,
        path: &[(String, bool)],
    ) -> Result<Option<KeywordId>, CatalogError> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(found) = self.found.get(path) {
            return Ok(*found);
        }
        let mut current = self.root;
        for (name, _) in path {
            let Some(parent) = current else { break };
            current = keywords::child(connection, Some(parent), name)?;
        }
        self.found.insert(path.to_vec(), current);
        Ok(current)
    }

    /// Mot-clé du chemin `path`, créé s'il le faut ; `None` pour un chemin
    /// vide.
    fn ensure(
        &mut self,
        connection: &Connection,
        path: &[(String, bool)],
    ) -> Result<Option<KeywordId>, CatalogError> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(Some(found)) = self.found.get(path) {
            return Ok(Some(*found));
        }
        let mut current = if let Some(root) = self.root {
            root
        } else {
            let root = create_root(connection)?;
            self.root = Some(root);
            root
        };
        for (name, is_missing_level) in path {
            current = if let Some(child) = keywords::child(connection, Some(current), name)? {
                child
            } else {
                let created = keywords::create(connection, Some(current), name)?;
                if *is_missing_level {
                    keyword_changes::set_include_on_export(connection, created, false)?;
                }
                created
            };
        }
        self.found.insert(path.to_vec(), Some(current));
        Ok(Some(current))
    }
}

/// « Lieux », s'il existe : celui retenu par le catalogue, à défaut un
/// mot-clé « Lieux » à la racine (créé avant, à la main), adopté.
fn existing_root(connection: &Connection) -> Result<Option<KeywordId>, CatalogError> {
    let stored: Option<i64> = connection
        .prepare_cached(
            "SELECT place_keywords.root_keyword_id FROM place_keywords
             JOIN keywords ON keywords.id = place_keywords.root_keyword_id
             WHERE place_keywords.id = 1",
        )?
        .query_row([], |row| row.get(0))
        .optional()?;
    if let Some(id) = stored {
        return Ok(Some(KeywordId::new(id)));
    }
    let adopted = keywords::child(connection, None, ROOT_NAME)?;
    if let Some(root) = adopted {
        remember_root(connection, root)?;
    }
    Ok(adopted)
}

fn create_root(connection: &Connection) -> Result<KeywordId, CatalogError> {
    let root = keywords::create(connection, None, ROOT_NAME)?;
    keyword_changes::set_include_on_export(connection, root, false)?;
    remember_root(connection, root)?;
    Ok(root)
}

fn remember_root(connection: &Connection, root: KeywordId) -> Result<(), CatalogError> {
    connection.execute(
        "UPDATE place_keywords SET root_keyword_id = ?1 WHERE id = 1",
        params![root.get()],
    )?;
    Ok(())
}
