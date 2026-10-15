//! Lecture du fichier annexe de DxO PhotoLab (`photo.NEF.dop`), **en
//! lecture seule**.
//!
//! Son format n'est pas documenté : c'est du texte à la manière de Lua,
//! fait de tables (`{ … }`) de valeurs nommées (`Nom = valeur,`) ou non,
//! de textes entre guillemets, de nombres et de booléens. Seuls les champs
//! observés dans de vrais fichiers sont lus : note, auteur, copyright,
//! orientation, date de prise de vue, mots-clés (sans hiérarchie : DxO
//! n'en a pas).

use std::path::Path;

use super::recovered::{KeywordPath, RecoveredMetadata, field_value};
use super::values::parse_iso_date;
use crate::domain::{MetadataField, Orientation, Rating, recovered_keyword_name};

/// Taille maximale d'un fichier `.dop` : les réglages de DxO en occupent
/// l'essentiel (une quinzaine de Kio pour une photo ordinaire).
const MAX_DOP_SIZE: u64 = 16 * 1024 * 1024;

/// Profondeur maximale des tables imbriquées (une dizaine dans les vrais
/// fichiers) : protège d'un fichier abîmé ou hostile.
const MAX_DEPTH: usize = 64;

/// Fichier `.dop` illisible.
#[derive(Debug, thiserror::Error)]
pub enum DopError {
    /// Le fichier n'a pas pu être lu.
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    /// Le fichier est démesuré : il n'est pas lu.
    #[error("DxO sidecar too large")]
    TooLarge,
    /// Le contenu ne suit pas la syntaxe attendue.
    #[error("malformed DxO sidecar: {0}")]
    Malformed(String),
}

/// Lit un fichier `.dop` ; `file_name` (`DSC_0002.NEF`) désigne la photo
/// parmi celles qu'il décrit (copies virtuelles).
///
/// # Errors
///
/// Voir [`DopError`].
pub fn read_dop(path: &Path, file_name: &str) -> Result<RecoveredMetadata, DopError> {
    if std::fs::metadata(path)?.len() > MAX_DOP_SIZE {
        return Err(DopError::TooLarge);
    }
    let bytes = std::fs::read(path)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| DopError::Malformed("not UTF-8".to_owned()))?;
    let root = parse(text.trim_start_matches('\u{feff}'))?;
    Ok(extract(&root, file_name))
}

/// Valeur du fichier.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
    Nil,
    Table(Table),
}

/// Table : valeurs nommées et valeurs à la suite.
#[derive(Debug, Clone, Default, PartialEq)]
struct Table {
    named: Vec<(String, Value)>,
    items: Vec<Value>,
}

impl Table {
    fn get(&self, name: &str) -> Option<&Value> {
        self.named
            .iter()
            .find(|(other, _)| other == name)
            .map(|(_, value)| value)
    }

    fn table(&self, name: &str) -> Option<&Self> {
        match self.get(name)? {
            Value::Table(table) => Some(table),
            _ => None,
        }
    }

    fn text(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    fn number(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            Value::Number(number) => Some(*number),
            _ => None,
        }
    }

    fn tables(&self) -> impl Iterator<Item = &Self> {
        self.items.iter().filter_map(|item| match item {
            Value::Table(table) => Some(table),
            _ => None,
        })
    }
}

/// Des valeurs lues aux métadonnées : la photo est l'élément de
/// `Sidecar.Source.Items` qui porte son nom, sinon le premier.
fn extract(root: &Table, file_name: &str) -> RecoveredMetadata {
    let items: Vec<&Table> = root
        .table("Sidecar")
        .and_then(|sidecar| sidecar.table("Source"))
        .and_then(|source| source.table("Items"))
        .map(|items| items.tables().collect())
        .unwrap_or_default();
    let Some(item) = items
        .iter()
        .find(|item| {
            item.text("Name")
                .is_some_and(|name| name.eq_ignore_ascii_case(file_name))
        })
        .or_else(|| items.first())
    else {
        return RecoveredMetadata::default();
    };

    let iptc = item.table("IPTC");
    let fields = [
        (MetadataField::Creator, "contactCreator"),
        (MetadataField::Copyright, "statusCopyrightNotice"),
    ]
    .into_iter()
    .filter_map(|(field, name)| Some((field, field_value(field, iptc?.text(name)?)?)))
    .collect();

    RecoveredMetadata {
        rating: item.number("Rating").and_then(stars),
        fields,
        orientation: item
            .number("Orientation")
            .and_then(small_integer)
            .and_then(Orientation::new),
        capture_time: item.text("ShotDate").and_then(parse_iso_date),
        keywords: item.table("Keywords").map_or_else(Vec::new, keywords),
        ..RecoveredMetadata::default()
    }
}

/// Note de 1 à 5 étoiles (0 ne dit rien, comme dans le XMP).
fn stars(value: f64) -> Option<Rating> {
    small_integer(value)
        .filter(|stars| (1..=Rating::MAX).contains(stars))
        .and_then(Rating::new)
}

fn small_integer(value: f64) -> Option<u8> {
    if value.fract() != 0.0 || !(0.0..=255.0).contains(&value) {
        return None;
    }
    // Conversion sûre : entier vérifié entre 0 et 255.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(value as u8)
}

/// Mots-clés : chaque élément est une table qui contient le nom. DxO
/// PhotoLab n'a pas de hiérarchie de mots-clés :
/// chaque nom est un mot-clé seul, même si une table en contenait
/// plusieurs.
fn keywords(table: &Table) -> Vec<KeywordPath> {
    let mut paths = Vec::new();
    for item in table.tables().flat_map(|entry| &entry.items) {
        let Value::Text(text) = item else {
            continue;
        };
        if let Some(path) = recovered_keyword_name(text).map(KeywordPath::Name)
            && !paths.contains(&path)
        {
            paths.push(path);
        }
    }
    paths
}

/// Lit le fichier : une suite de `Nom = valeur`, rangée dans une table.
fn parse(text: &str) -> Result<Table, DopError> {
    let mut parser = Parser {
        text,
        position: 0,
        depth: 0,
    };
    let mut root = Table::default();
    loop {
        parser.skip_blanks();
        if parser.position == text.len() {
            return Ok(root);
        }
        let name = parser.name()?;
        parser.expect('=')?;
        let value = parser.value()?;
        root.named.push((name, value));
        parser.skip_blanks();
        parser.eat(',');
    }
}

/// Lecteur à descente récursive, caractère par caractère.
struct Parser<'a> {
    text: &'a str,
    position: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn advance(&mut self, character: char) {
        self.position += character.len_utf8();
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance(expected);
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), DopError> {
        self.skip_blanks();
        if self.eat(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {expected:?}")))
        }
    }

    /// Espaces, retours à la ligne et commentaires Lua (`-- …`).
    fn skip_blanks(&mut self) {
        loop {
            let trimmed = self.rest().trim_start();
            self.position = self.text.len() - trimmed.len();
            if !trimmed.starts_with("--") {
                return;
            }
            self.position += trimmed.find('\n').unwrap_or(trimmed.len());
        }
    }

    fn name(&mut self) -> Result<String, DopError> {
        self.skip_blanks();
        let length = self
            .rest()
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(self.rest().len());
        if length == 0 || self.rest().starts_with(|c: char| c.is_ascii_digit()) {
            return Err(self.error("expected a name"));
        }
        let name = self.rest()[..length].to_owned();
        self.position += length;
        Ok(name)
    }

    fn value(&mut self) -> Result<Value, DopError> {
        self.skip_blanks();
        match self.peek() {
            Some('{') => self.table().map(Value::Table),
            Some('"') => self.string().map(Value::Text),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number().map(Value::Number),
            _ => match self.name()?.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "nil" => Ok(Value::Nil),
                _ => Err(self.error("unexpected word")),
            },
        }
    }

    fn table(&mut self) -> Result<Table, DopError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("tables nested too deeply"));
        }
        self.advance('{');
        let mut table = Table::default();
        loop {
            self.skip_blanks();
            if self.eat('}') {
                self.depth -= 1;
                return Ok(table);
            }
            if self.is_named_entry() {
                let name = self.name()?;
                self.expect('=')?;
                table.named.push((name, self.value()?));
            } else {
                table.items.push(self.value()?);
            }
            self.skip_blanks();
            if !self.eat(',') && self.peek() != Some('}') {
                return Err(self.error("expected ',' or '}'"));
            }
        }
    }

    /// Vrai si la suite est `Nom =` (et non une valeur seule).
    fn is_named_entry(&self) -> bool {
        let rest = self.rest();
        let length = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        length > 0
            && !rest.starts_with(|c: char| c.is_ascii_digit())
            && rest[length..].trim_start().starts_with('=')
    }

    fn string(&mut self) -> Result<String, DopError> {
        self.advance('"');
        let mut text = String::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error("unfinished text"));
            };
            self.advance(character);
            match character {
                '"' => return Ok(text),
                '\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(self.error("unfinished text"));
                    };
                    self.advance(escaped);
                    text.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                }
                other => text.push(other),
            }
        }
    }

    fn number(&mut self) -> Result<f64, DopError> {
        let length = self
            .rest()
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(self.rest().len());
        let number = self.rest()[..length]
            .parse::<f64>()
            .map_err(|_| self.error("invalid number"))?;
        self.position += length;
        Ok(number)
    }

    fn error(&self, message: &str) -> DopError {
        let line = self.text[..self.position].matches('\n').count() + 1;
        DopError::Malformed(format!("{message} at line {line}"))
    }
}
