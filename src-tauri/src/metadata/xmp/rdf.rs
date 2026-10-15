//! Lecture du RDF d'un paquet XMP : les propriétés de premier niveau de
//! chaque `rdf:Description`, écrites en attributs (`xmp:Rating="3"`, forme
//! compacte de Lightroom) ou en éléments (`<xmp:Rating>3</xmp:Rating>`), avec
//! les listes `rdf:Bag`, `rdf:Seq` et `rdf:Alt`.
//!
//! Les propriétés composées (structures, `rdf:parseType="Resource"`) sont
//! ignorées : aucun champ récupéré par PicLib n'en a besoin.

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, QName, ResolveResult};
use quick_xml::{NsReader, XmlVersion};

use super::XmpError;

use super::namespaces::{RDF, XML};

/// Langue par défaut d'une liste `rdf:Alt` (SPEC XMP, partie 1).
const DEFAULT_LANGUAGE: &str = "x-default";

/// Élément d'une liste.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    /// Langue (`xml:lang`), pour une liste `rdf:Alt`.
    language: Option<String>,
    /// Texte.
    text: String,
}

/// Valeur d'une propriété.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    /// Texte simple.
    Text(String),
    /// Liste (`rdf:Bag`, `rdf:Seq`) ou versions par langue (`rdf:Alt`).
    List(Vec<Item>),
}

/// Propriétés lues, repérées par leur espace de noms et leur nom local
/// (le préfixe varie d'un logiciel à l'autre : seul l'espace de noms compte).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    values: HashMap<(String, String), Value>,
}

impl Properties {
    /// Texte d'une propriété, sans espaces autour ; `None` s'il est vide.
    /// Pour une liste : la version `x-default`, sinon le premier élément.
    pub fn text(&self, namespace: &str, name: &str) -> Option<&str> {
        let text = match self.get(namespace, name)? {
            Value::Text(text) => text.as_str(),
            Value::List(items) => items
                .iter()
                .find(|item| item.language.as_deref() == Some(DEFAULT_LANGUAGE))
                .or_else(|| items.first())
                .map(|item| item.text.as_str())?,
        };
        let text = text.trim();
        (!text.is_empty()).then_some(text)
    }

    /// Éléments non vides d'une liste, sans espaces autour ; un texte
    /// simple compte comme une liste d'un élément.
    pub fn list(&self, namespace: &str, name: &str) -> Vec<&str> {
        let texts: Vec<&str> = match self.get(namespace, name) {
            None => Vec::new(),
            Some(Value::Text(text)) => vec![text.as_str()],
            Some(Value::List(items)) => items.iter().map(|item| item.text.as_str()).collect(),
        };
        texts
            .into_iter()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect()
    }

    fn get(&self, namespace: &str, name: &str) -> Option<&Value> {
        self.values.get(&(namespace.to_owned(), name.to_owned()))
    }

    /// Garde la première valeur d'une propriété présente deux fois.
    fn insert(&mut self, key: (String, String), value: Value) {
        self.values.entry(key).or_insert(value);
    }
}

/// Lit les propriétés d'un paquet XMP.
///
/// # Errors
///
/// Échoue si le XML est mal formé.
pub fn parse(xml: &str) -> Result<Properties, XmpError> {
    let mut reader = NsReader::from_str(xml);
    let mut properties = Properties::default();
    // Éléments ouverts autour des `rdf:Description` (`x:xmpmeta`,
    // `rdf:RDF`) : un fichier tronqué se termine avant de les fermer.
    let mut open_elements = 0_usize;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(malformed)?;
        let is_description = is_rdf(&namespace);
        match event {
            Event::Start(start) if is_description && is_named(&start, "Description") => {
                read_attributes(&reader, &start, &mut properties)?;
                read_description(&mut reader, &mut properties)?;
            }
            Event::Empty(start) if is_description && is_named(&start, "Description") => {
                read_attributes(&reader, &start, &mut properties)?;
            }
            Event::Start(_) => open_elements += 1,
            Event::End(_) => open_elements = open_elements.saturating_sub(1),
            Event::Eof if open_elements > 0 => return Err(unexpected_end()),
            Event::Eof => return Ok(properties),
            _ => {}
        }
    }
}

/// Propriétés écrites en attributs d'un `rdf:Description`.
fn read_attributes(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
    properties: &mut Properties,
) -> Result<(), XmpError> {
    for attribute in start.attributes() {
        let attribute = attribute.map_err(malformed)?;
        if attribute.key.as_namespace_binding().is_some() {
            continue;
        }
        let (ResolveResult::Bound(Namespace(namespace)), name) =
            reader.resolver().resolve_attribute(attribute.key)
        else {
            continue;
        };
        if namespace == RDF || namespace == XML {
            continue;
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(malformed)?;
        properties.insert(
            (namespace.to_owned(), name.as_ref().to_owned()),
            Value::Text(value.into_owned()),
        );
    }
    Ok(())
}

/// Propriétés écrites en éléments, jusqu'à la fin du `rdf:Description`.
fn read_description(
    reader: &mut NsReader<&[u8]>,
    properties: &mut Properties,
) -> Result<(), XmpError> {
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(malformed)?;
        let key = match &namespace {
            ResolveResult::Bound(Namespace(namespace)) => Some((*namespace).to_owned()),
            _ => None,
        };
        match event {
            Event::Start(start) => {
                let value = if is_structure(&start)? {
                    skip(reader, start.name())?;
                    None
                } else {
                    read_property(reader)?
                };
                if let (Some(namespace), Some(value)) = (key, value) {
                    let name = start.local_name().as_ref().to_owned();
                    properties.insert((namespace, name), value);
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(unexpected_end()),
            // Propriété vide ou désignant une ressource (`rdf:resource`),
            // espaces, commentaires : rien à garder.
            _ => {}
        }
    }
}

/// Valeur d'une propriété, jusqu'à sa balise de fin : un texte, ou une
/// liste ; `None` pour une structure.
fn read_property(reader: &mut NsReader<&[u8]>) -> Result<Option<Value>, XmpError> {
    let mut text = String::new();
    let mut list = None;
    let mut is_structure = false;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(malformed)?;
        let is_container = is_rdf(&namespace);
        match event {
            Event::Start(start) if is_container && is_list(&start) => {
                list = Some(read_list(reader)?);
            }
            Event::Empty(start) if is_container && is_list(&start) => list = Some(Vec::new()),
            Event::Start(start) => {
                skip(reader, start.name())?;
                is_structure = true;
            }
            Event::Empty(_) => is_structure = true,
            Event::End(_) => {
                return Ok(if is_structure {
                    None
                } else {
                    Some(list.map_or(Value::Text(text), Value::List))
                });
            }
            Event::Eof => return Err(unexpected_end()),
            other => push_text(&mut text, &other)?,
        }
    }
}

/// Éléments `rdf:li` d'une liste, jusqu'à sa balise de fin. Un élément
/// composé (structure) est ignoré.
fn read_list(reader: &mut NsReader<&[u8]>) -> Result<Vec<Item>, XmpError> {
    let mut items = Vec::new();
    loop {
        let (_, event) = reader.read_resolved_event().map_err(malformed)?;
        match event {
            Event::Start(start) => {
                let language = language(&start)?;
                if is_structure(&start)? {
                    skip(reader, start.name())?;
                } else if let Some(text) = read_text(reader)? {
                    items.push(Item { language, text });
                }
            }
            Event::Empty(start) => items.push(Item {
                language: language(&start)?,
                text: String::new(),
            }),
            Event::End(_) => return Ok(items),
            Event::Eof => return Err(unexpected_end()),
            _ => {}
        }
    }
}

/// Texte d'un élément, jusqu'à sa balise de fin ; `None` s'il contient
/// d'autres éléments.
fn read_text(reader: &mut NsReader<&[u8]>) -> Result<Option<String>, XmpError> {
    let mut text = String::new();
    let mut has_elements = false;
    loop {
        let event = reader.read_event().map_err(malformed)?;
        match event {
            Event::Start(start) => {
                skip(reader, start.name())?;
                has_elements = true;
            }
            Event::Empty(_) => has_elements = true,
            Event::End(_) => return Ok((!has_elements).then_some(text)),
            Event::Eof => return Err(unexpected_end()),
            other => push_text(&mut text, &other)?,
        }
    }
}

/// Ajoute à `text` le texte d'un événement (texte, section CDATA ou
/// référence comme `&amp;` ou `&#233;`).
fn push_text(text: &mut String, event: &Event<'_>) -> Result<(), XmpError> {
    match event {
        Event::Text(content) => text.push_str(&content.xml10_content()),
        Event::CData(content) => text.push_str(&content.xml10_content()),
        Event::GeneralRef(reference) => {
            if let Some(character) = reference.resolve_char_ref().map_err(malformed)? {
                text.push(character);
            } else if let Some(replacement) =
                quick_xml::escape::resolve_predefined_entity(&reference.xml10_content())
            {
                text.push_str(replacement);
            } else {
                return Err(XmpError::Malformed(format!(
                    "unknown entity &{};",
                    reference.xml10_content()
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Saute un élément et tout son contenu.
fn skip(reader: &mut NsReader<&[u8]>, name: QName<'_>) -> Result<(), XmpError> {
    reader.read_to_end(name).map(|_| ()).map_err(malformed)
}

/// Vrai pour un élément composé : `rdf:parseType="Resource"`.
fn is_structure(start: &BytesStart<'_>) -> Result<bool, XmpError> {
    Ok(start
        .try_get_attribute("rdf:parseType")
        .map_err(malformed)?
        .is_some_and(|attribute| attribute.value == "Resource"))
}

/// Langue d'un élément de liste (`xml:lang`).
fn language(start: &BytesStart<'_>) -> Result<Option<String>, XmpError> {
    Ok(start
        .try_get_attribute("xml:lang")
        .map_err(malformed)?
        .map(|attribute| attribute.value.into_owned()))
}

fn is_rdf(namespace: &ResolveResult<'_>) -> bool {
    matches!(namespace, ResolveResult::Bound(Namespace(namespace)) if *namespace == RDF)
}

fn is_named(start: &BytesStart<'_>, name: &str) -> bool {
    start.local_name().as_ref() == name
}

fn is_list(start: &BytesStart<'_>) -> bool {
    matches!(start.local_name().as_ref(), "Bag" | "Seq" | "Alt")
}

fn malformed(error: impl std::fmt::Display) -> XmpError {
    XmpError::Malformed(error.to_string())
}

fn unexpected_end() -> XmpError {
    XmpError::Malformed("unexpected end of the XMP packet".to_owned())
}
