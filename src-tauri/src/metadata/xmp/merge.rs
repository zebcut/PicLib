//! Fusion d'un paquet XMP existant avec ce que PicLib écrit.
//!
//! Le fichier est relu élément par élément et recopié tel quel, à deux
//! exceptions près : les propriétés tenues par PicLib (voir `content`) sont
//! retirées de chaque `rdf:Description` de premier niveau, en attributs
//! comme en éléments, puis réécrites à la fin de la première. Les réglages
//! de développement de Camera Raw, l'historique de darktable, les
//! structures, les commentaires restent octet pour octet.
//!
//! [`canonical`] décrit un paquet sans les propriétés de PicLib : un paquet
//! fusionné doit avoir la même description que le paquet d'origine, sinon
//! il n'est pas écrit.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::name::{Namespace, QName, ResolveResult};
use quick_xml::{NsReader, Writer};

use super::XmpError;
use super::content::{Property, PropertyValue};
use super::namespaces::{PREFIXES, RDF};

/// Retrait d'un niveau, comme dans les fichiers d'Adobe.
const INDENT: &str = "   ";

/// Paquet d'un fichier neuf : une description vide, que la fusion remplit.
pub(super) const EMPTY_PACKET: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="PicLib">
   <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about=""/>
   </rdf:RDF>
</x:xmpmeta>
"#;

/// Propriétés tenues par PicLib (espace de noms, nom local).
pub(super) type Owned<'a> = &'a [(&'static str, &'static str)];

/// Paquet `xml` dont les propriétés `owned` sont remplacées par `properties`.
///
/// # Errors
///
/// [`XmpError::Malformed`] si le XML est mal formé ou n'a pas de `rdf:RDF`.
pub(super) fn merge(
    xml: &str,
    owned: Owned<'_>,
    properties: &[Property],
) -> Result<String, XmpError> {
    let scope = scan(xml)?;
    let prefixes = assign_prefixes(&scope, properties);
    let mut rewrite = Rewrite {
        reader: NsReader::from_str(xml),
        writer: Writer::new(Vec::new()),
        owned,
        properties,
        prefixes: &prefixes,
        rdf_prefix: &scope.rdf_prefix,
        level: 0,
        rdf_level: None,
        description: None,
        description_first: false,
        injected: false,
        held: String::new(),
    };
    rewrite.run()?;
    String::from_utf8(rewrite.writer.into_inner()).map_err(malformed)
}

/// Description d'un paquet sans les propriétés `owned` ni les déclarations
/// des espaces de noms de PicLib : la liste des éléments, attributs triés,
/// sans les espaces entre éléments. Une description de premier niveau qui
/// n'a plus rien d'autre que `rdf:about` est omise : elle ne dit rien.
///
/// # Errors
///
/// [`XmpError::Malformed`] si le XML est mal formé.
pub(super) fn canonical(xml: &str, owned: Owned<'_>) -> Result<Vec<String>, XmpError> {
    let ours: HashSet<&str> = PREFIXES.iter().map(|(namespace, _)| *namespace).collect();
    let mut reader = NsReader::from_str(xml);
    let mut tokens = Vec::new();
    let mut level = 0_usize;
    let mut rdf_level = None;
    let mut description: Option<usize> = None;
    // Description de premier niveau ouverte sans rien d'autre que
    // `rdf:about` : position de son jeton, retiré si elle reste vide.
    let mut bare_description: Option<usize> = None;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(malformed)?;
        let namespace = namespace_of(&namespace);
        let (start, is_empty) = match event {
            Event::Start(start) => (start, false),
            Event::Empty(start) => (start, true),
            Event::End(end) => {
                level = level.saturating_sub(1);
                if description == Some(level) {
                    description = None;
                    if let Some(index) = bare_description.take()
                        && tokens.len() == index + 1
                    {
                        tokens.truncate(index);
                        continue;
                    }
                }
                tokens.push(format!("</{}>", end.name().0));
                continue;
            }
            Event::Text(text) => {
                let raw: &str = &text;
                if !raw.trim().is_empty() {
                    tokens.push(raw.to_owned());
                }
                continue;
            }
            Event::Eof => return Ok(tokens),
            other => {
                tokens.push(format!("{other:?}"));
                continue;
            }
        };
        let this_level = level;
        let is_rdf = namespace.as_deref() == Some(RDF);
        if is_rdf && is_named(&start, "RDF") && rdf_level.is_none() {
            rdf_level = Some(this_level);
        }
        let is_top_description = description.is_none()
            && is_rdf
            && is_named(&start, "Description")
            && rdf_level.is_some_and(|rdf| rdf + 1 == this_level);
        let is_owned_child = description.is_some_and(|depth| depth + 1 == this_level)
            && is_owned_element(namespace.as_deref(), &start, owned);
        if is_owned_child {
            if !is_empty {
                reader.read_to_end(start.name()).map_err(malformed)?;
            }
            continue;
        }
        let mut attributes = Vec::new();
        for attribute in start.attributes() {
            let attribute = attribute.map_err(malformed)?;
            let is_ours = if attribute.key.as_namespace_binding().is_some() {
                ours.contains(attribute.value.as_ref())
            } else {
                is_owned_attribute(&reader, &attribute, owned)
            };
            if is_top_description && is_ours {
                continue;
            }
            attributes.push(format!("{}={}", attribute.key.0, attribute.value));
        }
        attributes.sort();
        let is_bare = is_top_description
            && attributes
                .iter()
                .all(|attribute| attribute.ends_with(":about=") || attribute == "about=");
        if is_bare && is_empty {
            continue;
        }
        if is_bare {
            bare_description = Some(tokens.len());
        }
        tokens.push(format!("<{} {}>", start.name().0, attributes.join(" ")));
        if is_empty {
            tokens.push(format!("</{}>", start.name().0));
        } else {
            level += 1;
            if is_top_description {
                description = Some(this_level);
            }
        }
    }
}

/// Ce qu'il faut savoir du paquet avant de le réécrire.
#[derive(Debug, Default)]
struct Scope {
    /// Préfixe du RDF (celui de `rdf:RDF`).
    rdf_prefix: String,
    /// Préfixes déclarés autour de la première description, et par elle,
    /// avec leur espace de noms (le plus proche l'emporte).
    bindings: HashMap<String, String>,
    /// Tous les préfixes déclarés dans le paquet.
    declared: HashSet<String>,
}

/// Repère le préfixe du RDF et les déclarations d'espaces de noms.
fn scan(xml: &str) -> Result<Scope, XmpError> {
    let mut reader = NsReader::from_str(xml);
    let mut scope = Scope::default();
    let mut stack: Vec<Vec<(String, String)>> = Vec::new();
    let mut rdf_level = None;
    let mut found = false;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(malformed)?;
        let (start, is_empty) = match event {
            Event::Start(start) => (start, false),
            Event::Empty(start) => (start, true),
            Event::End(_) => {
                stack.pop();
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let declarations = declarations(&start)?;
        for (prefix, _) in &declarations {
            scope.declared.insert(prefix.clone());
        }
        let level = stack.len();
        let is_rdf = is_bound_to(&namespace, RDF);
        let is_root = is_rdf && is_named(&start, "RDF") && rdf_level.is_none();
        let is_first_description = !found
            && is_rdf
            && is_named(&start, "Description")
            && rdf_level.is_some_and(|rdf| rdf + 1 == level);
        if is_root {
            rdf_level = Some(level);
            scope.rdf_prefix = start
                .name()
                .prefix()
                .map(|prefix| prefix.as_ref().to_owned())
                .unwrap_or_default();
        }
        // Sans description, les nouvelles propriétés iront dans une
        // description créée sous `rdf:RDF` : ses déclarations comptent.
        if is_root || is_first_description {
            found |= is_first_description;
            scope.bindings = stack
                .iter()
                .flatten()
                .chain(&declarations)
                .cloned()
                .collect();
        }
        if !is_empty {
            stack.push(declarations);
        }
    }
    if rdf_level.is_none() {
        return Err(XmpError::Malformed("no rdf:RDF element".to_owned()));
    }
    Ok(scope)
}

/// Déclarations de préfixes nommés d'un élément (`xmlns:dc="..."`).
fn declarations(start: &BytesStart<'_>) -> Result<Vec<(String, String)>, XmpError> {
    let mut declarations = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(malformed)?;
        if let Some(quick_xml::name::PrefixDeclaration::Named(prefix)) =
            attribute.key.as_namespace_binding()
        {
            declarations.push((prefix.to_owned(), attribute.value.into_owned()));
        }
    }
    Ok(declarations)
}

/// Préfixe de chaque espace de noms des propriétés à écrire : celui qui le
/// désigne déjà autour de la première description, sinon le préfixe
/// habituel, ou un préfixe libre ; et les déclarations à y ajouter.
struct Prefixes {
    by_namespace: HashMap<&'static str, String>,
    to_declare: Vec<(String, &'static str)>,
}

fn assign_prefixes(scope: &Scope, properties: &[Property]) -> Prefixes {
    let mut prefixes = Prefixes {
        by_namespace: HashMap::new(),
        to_declare: Vec::new(),
    };
    let mut taken = scope.declared.clone();
    for property in properties {
        let namespace = property.namespace;
        if prefixes.by_namespace.contains_key(namespace) {
            continue;
        }
        let bound = scope
            .bindings
            .iter()
            .filter(|(prefix, uri)| uri.as_str() == namespace && !prefix.is_empty())
            .map(|(prefix, _)| prefix.clone())
            .min();
        let prefix = bound.unwrap_or_else(|| {
            let usual = PREFIXES
                .iter()
                .find(|(uri, _)| *uri == namespace)
                .map_or("piclib", |(_, prefix)| *prefix);
            let prefix = if taken.contains(usual) {
                (1..=u32::MAX)
                    .map(|number| format!("{usual}{number}"))
                    .find(|candidate| !taken.contains(candidate))
                    .unwrap_or_default()
            } else {
                usual.to_owned()
            };
            taken.insert(prefix.clone());
            prefixes.to_declare.push((prefix.clone(), namespace));
            prefix
        });
        prefixes.by_namespace.insert(namespace, prefix);
    }
    prefixes
}

/// Réécriture en cours.
struct Rewrite<'x, 'p> {
    reader: NsReader<&'x [u8]>,
    writer: Writer<Vec<u8>>,
    owned: Owned<'p>,
    properties: &'p [Property],
    prefixes: &'p Prefixes,
    rdf_prefix: &'p str,
    /// Nombre d'éléments ouverts.
    level: usize,
    /// Niveau de `rdf:RDF`.
    rdf_level: Option<usize>,
    /// Description de premier niveau ouverte : son niveau.
    description: Option<usize>,
    /// La description ouverte est la première : elle reçoit les propriétés.
    description_first: bool,
    /// Les propriétés de PicLib sont écrites.
    injected: bool,
    /// Espaces entre deux propriétés d'une description, gardés jusqu'à
    /// savoir si la propriété suivante est recopiée ou retirée.
    held: String,
}

impl Rewrite<'_, '_> {
    fn run(&mut self) -> Result<(), XmpError> {
        loop {
            let (namespace, event) = self.reader.read_resolved_event().map_err(malformed)?;
            let namespace = namespace_of(&namespace);
            match event {
                Event::Start(start) => self.start(namespace.as_deref(), start, false)?,
                Event::Empty(start) => self.start(namespace.as_deref(), start, true)?,
                Event::End(end) => self.end(end)?,
                Event::Text(text) if self.is_between_properties() && text.trim().is_empty() => {
                    let raw: &str = &text;
                    self.held.push_str(raw);
                }
                Event::Eof => break,
                other => {
                    self.flush();
                    self.write(other)?;
                }
            }
        }
        if self.injected {
            Ok(())
        } else {
            Err(XmpError::Malformed("no rdf:RDF element".to_owned()))
        }
    }

    fn is_between_properties(&self) -> bool {
        self.description
            .is_some_and(|depth| depth + 1 == self.level)
    }

    fn start(
        &mut self,
        namespace: Option<&str>,
        start: BytesStart<'_>,
        is_empty: bool,
    ) -> Result<(), XmpError> {
        let level = self.level;
        let is_rdf = namespace == Some(RDF);
        if self.description.is_none()
            && is_rdf
            && is_named(&start, "RDF")
            && self.rdf_level.is_none()
        {
            self.rdf_level = Some(level);
        }
        if self.description.is_none()
            && is_rdf
            && is_named(&start, "Description")
            && self.rdf_level.is_some_and(|rdf| rdf + 1 == level)
        {
            return self.description_start(&start, is_empty, level);
        }
        if self.is_between_properties() && is_owned_element(namespace, &start, self.owned) {
            // Propriété de PicLib : retirée, avec les espaces qui la
            // précèdent.
            self.held.clear();
            if !is_empty {
                self.reader.read_to_end(start.name()).map_err(malformed)?;
            }
            return Ok(());
        }
        self.flush();
        if is_empty {
            self.write(Event::Empty(start))
        } else {
            self.level += 1;
            self.write(Event::Start(start))
        }
    }

    fn description_start(
        &mut self,
        start: &BytesStart<'_>,
        is_empty: bool,
        level: usize,
    ) -> Result<(), XmpError> {
        self.flush();
        let first = !self.injected;
        let rebuilt = self.strip_attributes(start, first)?;
        if !is_empty {
            self.level += 1;
            self.description = Some(level);
            self.description_first = first;
            return self.write(Event::Start(rebuilt));
        }
        if !first {
            return self.write(Event::Empty(rebuilt));
        }
        // Description vide : elle reçoit les propriétés de PicLib.
        let name = rebuilt.name().0.to_owned();
        self.write(Event::Start(rebuilt))?;
        self.inject(level);
        self.write(Event::End(BytesEnd::new(name)))
    }

    fn end(&mut self, end: BytesEnd<'_>) -> Result<(), XmpError> {
        self.level = self.level.saturating_sub(1);
        let level = self.level;
        if self.description == Some(level) {
            self.description = None;
            if self.description_first && !self.injected {
                self.inject(level);
            } else {
                self.flush();
            }
            return self.write(Event::End(end));
        }
        if self.rdf_level == Some(level) && !self.injected {
            self.create_description(level)?;
        }
        self.flush();
        self.write(Event::End(end))
    }

    /// Aucune description sous `rdf:RDF` (niveau `level`) : en crée une.
    fn create_description(&mut self, level: usize) -> Result<(), XmpError> {
        self.raw(INDENT);
        let mut start = BytesStart::new(qualified(self.rdf_prefix, "Description"));
        start.push_attribute((qualified(self.rdf_prefix, "about").as_str(), ""));
        for (prefix, namespace) in &self.prefixes.to_declare {
            start.push_attribute((format!("xmlns:{prefix}").as_str(), *namespace));
        }
        let name = start.name().0.to_owned();
        self.write(Event::Start(start))?;
        self.inject(level + 1);
        self.write(Event::End(BytesEnd::new(name)))?;
        self.raw(&format!("\n{}", INDENT.repeat(level)));
        Ok(())
    }

    /// Recopie l'élément d'ouverture d'une description sans les propriétés
    /// de PicLib écrites en attributs ; la première reçoit les
    /// déclarations des espaces de noms à ajouter.
    fn strip_attributes(
        &self,
        start: &BytesStart<'_>,
        first: bool,
    ) -> Result<BytesStart<'static>, XmpError> {
        let mut rebuilt = BytesStart::new(start.name().0.to_owned());
        for attribute in start.attributes() {
            let attribute = attribute.map_err(malformed)?;
            if attribute.key.as_namespace_binding().is_none()
                && is_owned_attribute(&self.reader, &attribute, self.owned)
            {
                continue;
            }
            // Valeur brute, déjà échappée ; un `"` permis entre apostrophes
            // ne l'est plus entre guillemets.
            let value = attribute.value.replace('"', "&quot;");
            rebuilt.push_attribute(Attribute {
                key: QName(attribute.key.0),
                value: Cow::Owned(value),
            });
        }
        if first {
            for (prefix, namespace) in &self.prefixes.to_declare {
                rebuilt.push_attribute((format!("xmlns:{prefix}").as_str(), *namespace));
            }
        }
        Ok(rebuilt.into_owned())
    }

    /// Écrit les propriétés de PicLib dans la description du niveau
    /// `level`, puis les espaces de sa balise de fin.
    fn inject(&mut self, level: usize) {
        let mut text = String::new();
        for property in self.properties {
            let prefix = self
                .prefixes
                .by_namespace
                .get(property.namespace)
                .map_or("", String::as_str);
            write_property(&mut text, prefix, self.rdf_prefix, property, level + 1);
        }
        text.push('\n');
        text.push_str(&INDENT.repeat(level));
        self.held.clear();
        self.injected = true;
        self.raw(&text);
    }

    fn flush(&mut self) {
        let held = std::mem::take(&mut self.held);
        self.raw(&held);
    }

    fn raw(&mut self, text: &str) {
        self.writer.get_mut().extend_from_slice(text.as_bytes());
    }

    fn write(&mut self, event: Event<'_>) -> Result<(), XmpError> {
        self.writer.write_event(event).map_err(malformed)
    }
}

/// Écrit une propriété en élément, au niveau `level`.
fn write_property(out: &mut String, prefix: &str, rdf: &str, property: &Property, level: usize) {
    let name = qualified(prefix, property.name);
    new_line(out, level);
    match &property.value {
        PropertyValue::Text(text) => {
            push_all(out, &["<", &name, ">", &escape(text), "</", &name, ">"]);
        }
        PropertyValue::Alt(text) => {
            write_list(out, &name, rdf, "Alt", std::slice::from_ref(text), level);
        }
        PropertyValue::Bag(items) => write_list(out, &name, rdf, "Bag", items, level),
        PropertyValue::Seq(items) => write_list(out, &name, rdf, "Seq", items, level),
    }
}

fn write_list(out: &mut String, name: &str, rdf: &str, kind: &str, items: &[String], level: usize) {
    let container = qualified(rdf, kind);
    let item = qualified(rdf, "li");
    let language = if kind == "Alt" {
        r#" xml:lang="x-default""#
    } else {
        ""
    };
    push_all(out, &["<", name, ">"]);
    new_line(out, level + 1);
    push_all(out, &["<", &container, ">"]);
    for text in items {
        new_line(out, level + 2);
        push_all(
            out,
            &["<", &item, language, ">", &escape(text), "</", &item, ">"],
        );
    }
    new_line(out, level + 1);
    push_all(out, &["</", &container, ">"]);
    new_line(out, level);
    push_all(out, &["</", name, ">"]);
}

/// Passe à la ligne, avec le retrait du niveau `level`.
fn new_line(out: &mut String, level: usize) {
    out.push('\n');
    out.push_str(&INDENT.repeat(level));
}

fn push_all(out: &mut String, parts: &[&str]) {
    for part in parts {
        out.push_str(part);
    }
}

/// Nom avec son préfixe (`dc:title`), ou seul pour l'espace de noms par
/// défaut.
fn qualified(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}:{name}")
    }
}

/// Espace de noms résolu, recopié : il ne doit plus emprunter le lecteur.
fn namespace_of(resolved: &ResolveResult<'_>) -> Option<String> {
    match resolved {
        ResolveResult::Bound(Namespace(namespace)) => Some((*namespace).to_owned()),
        _ => None,
    }
}

fn is_bound_to(resolved: &ResolveResult<'_>, uri: &str) -> bool {
    matches!(resolved, ResolveResult::Bound(Namespace(namespace)) if *namespace == uri)
}

fn is_named(start: &BytesStart<'_>, name: &str) -> bool {
    start.local_name().as_ref() == name
}

fn is_owned_element(namespace: Option<&str>, start: &BytesStart<'_>, owned: Owned<'_>) -> bool {
    namespace.is_some_and(|namespace| {
        owned
            .iter()
            .any(|(uri, name)| *uri == namespace && is_named(start, name))
    })
}

fn is_owned_attribute(
    reader: &NsReader<&[u8]>,
    attribute: &Attribute<'_>,
    owned: Owned<'_>,
) -> bool {
    let (resolved, local) = reader.resolver().resolve_attribute(attribute.key);
    let ResolveResult::Bound(Namespace(namespace)) = resolved else {
        return false;
    };
    owned
        .iter()
        .any(|(uri, name)| *uri == namespace && local.as_ref() == *name)
}

fn escape(text: &str) -> Cow<'_, str> {
    quick_xml::escape::escape(text)
}

fn malformed(error: impl std::fmt::Display) -> XmpError {
    XmpError::Malformed(error.to_string())
}
