//! Ce que PicLib écrit dans un fichier XMP : note,
//! marqueur, couleur, champs descriptifs et lieu, mots-clés,
//! GPS, orientation, date de prise de vue si elle a été modifiée.
//!
//! PicLib est « propriétaire » de ces propriétés : à l'écriture, elles
//! prennent la valeur du catalogue (ou disparaissent si elle est vide).
//! Toutes les autres propriétés du fichier sont gardées telles quelles.

use super::fields::LEVEL_SEPARATOR;
use super::namespaces::{DC, EXIF, IPTC_CORE, LIGHTROOM, PHOTOSHOP, PICLIB, TIFF, XMP};
use crate::domain::{ColorLabel, Flag, GpsPosition, MetadataField, Orientation};

/// Propriétés toujours tenues par PicLib.
const ALWAYS_OWNED: [(&str, &str); 20] = [
    (XMP, "Rating"),
    (XMP, "Label"),
    (XMP, "MetadataDate"),
    (PICLIB, "Pick"),
    (DC, "title"),
    (DC, "description"),
    (DC, "creator"),
    (DC, "rights"),
    (DC, "subject"),
    (LIGHTROOM, "hierarchicalSubject"),
    (PHOTOSHOP, "Country"),
    (PHOTOSHOP, "State"),
    (PHOTOSHOP, "City"),
    (PHOTOSHOP, "SidecarForExtension"),
    (IPTC_CORE, "Location"),
    (EXIF, "GPSLatitude"),
    (EXIF, "GPSLongitude"),
    (EXIF, "GPSAltitude"),
    (EXIF, "GPSAltitudeRef"),
    (TIFF, "Orientation"),
];

/// Propriétés de la date de prise de vue, tenues seulement quand elle a été
/// modifiée dans PicLib : sinon celles du fichier restent.
const DATE_PROPERTIES: [(&str, &str); 2] = [(EXIF, "DateTimeOriginal"), (PHOTOSHOP, "DateCreated")];

/// Métadonnées d'un média à écrire dans son fichier XMP.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmpContent {
    /// Note, de 0 à 5 étoiles.
    pub rating: u8,
    /// Marqueur.
    pub flag: Option<Flag>,
    /// Étiquette de couleur.
    pub color_label: Option<ColorLabel>,
    /// Champs descriptifs non vides (titre, légende, auteur, copyright,
    /// pays, région, ville, lieu précis).
    pub fields: Vec<(MetadataField, String)>,
    /// Position GPS.
    pub gps: Option<GpsPosition>,
    /// Orientation à l'affichage : celle du fichier, avec la rotation PicLib.
    pub orientation: Option<Orientation>,
    /// Date de prise de vue modifiée dans PicLib, au format ISO 8601
    /// (« 2024-07-14T10:00:00+02:00 ») ; `None` : celle du fichier.
    pub capture_time: Option<String>,
    /// Chemin complet de chaque mot-clé du média, depuis la racine
    /// (champ hiérarchique de Lightroom).
    pub keyword_paths: Vec<Vec<String>>,
    /// Liste simple : les mots-clés inclus à l'export et leurs parents
    /// inclus à l'export, chacun une fois.
    pub subjects: Vec<String>,
    /// Extension de la photo décrite, pour un `photo.xmp` partagé par un RAW
    /// et un JPEG du même nom (« NEF »).
    pub sidecar_for_extension: Option<String>,
    /// Date de cette écriture, au format ISO 8601 (`xmp:MetadataDate`,
    /// que Lightroom compare à son catalogue).
    pub metadata_date: String,
}

/// Caractère permis dans un document XML 1.0.
fn is_xml_char(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

impl XmpContent {
    /// Les mêmes valeurs, sans les caractères qu'XML 1.0 interdit
    /// (contrôles, sauf tabulation et retours à la ligne) : ils rendraient
    /// le fichier illisible pour tous les logiciels (un titre collé, un EXIF
    /// abîmé...). Une valeur qui n'en contenait que d'autres disparaît.
    #[must_use]
    pub fn xml_safe(&self) -> Self {
        let clean = |text: &str| -> String { text.chars().filter(|c| is_xml_char(*c)).collect() };
        Self {
            fields: self
                .fields
                .iter()
                .map(|(field, value)| (*field, clean(value)))
                .filter(|(_, value)| !value.trim().is_empty())
                .collect(),
            capture_time: self.capture_time.as_deref().map(clean),
            keyword_paths: self
                .keyword_paths
                .iter()
                .map(|path| {
                    path.iter()
                        .map(|name| clean(name))
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                })
                .filter(|path| !path.is_empty())
                .collect(),
            subjects: self
                .subjects
                .iter()
                .map(|subject| clean(subject))
                .filter(|subject| !subject.is_empty())
                .collect(),
            sidecar_for_extension: self.sidecar_for_extension.as_deref().map(clean),
            metadata_date: clean(&self.metadata_date),
            ..self.clone()
        }
    }

    /// Valeur d'un champ descriptif.
    #[must_use]
    pub fn field(&self, field: MetadataField) -> Option<&str> {
        self.fields
            .iter()
            .find(|(other, _)| *other == field)
            .map(|(_, value)| value.as_str())
    }
}

/// Valeur d'une propriété écrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PropertyValue {
    /// Texte simple.
    Text(String),
    /// Liste non ordonnée (`rdf:Bag`).
    Bag(Vec<String>),
    /// Liste ordonnée (`rdf:Seq`).
    Seq(Vec<String>),
    /// Texte dans la langue par défaut (`rdf:Alt`, `x-default`).
    Alt(String),
}

/// Propriété écrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Property {
    pub namespace: &'static str,
    pub name: &'static str,
    pub value: PropertyValue,
}

/// Propriétés tenues par PicLib pour ce contenu (espace de noms, nom).
pub(super) fn owned(content: &XmpContent) -> Vec<(&'static str, &'static str)> {
    let mut owned = ALWAYS_OWNED.to_vec();
    if content.capture_time.is_some() {
        owned.extend(DATE_PROPERTIES);
    }
    owned
}

/// Propriété à écrire si sa valeur existe.
type Entry = (&'static str, &'static str, Option<PropertyValue>);

/// Propriétés à écrire, dans l'ordre du fichier.
pub(super) fn properties(content: &XmpContent) -> Vec<Property> {
    let text = |field| content.field(field).map(str::to_owned);
    let mut entries: Vec<Entry> = vec![
        (
            XMP,
            "Rating",
            Some(PropertyValue::Text(content.rating.to_string())),
        ),
        (
            XMP,
            "Label",
            content
                .color_label
                .map(|label| PropertyValue::Text(label_name(label).to_owned())),
        ),
        (
            PICLIB,
            "Pick",
            content
                .flag
                .map(|flag| PropertyValue::Text(pick_name(flag).to_owned())),
        ),
        (
            DC,
            "title",
            text(MetadataField::Title).map(PropertyValue::Alt),
        ),
        (
            DC,
            "description",
            text(MetadataField::Caption).map(PropertyValue::Alt),
        ),
        // Un seul élément, comme Lightroom : la lecture réunit les auteurs
        // d'une liste par « ; », découper ici changerait le texte relu.
        (
            DC,
            "creator",
            text(MetadataField::Creator).map(|creator| PropertyValue::Seq(vec![creator])),
        ),
        (
            DC,
            "rights",
            text(MetadataField::Copyright).map(PropertyValue::Alt),
        ),
        (
            PHOTOSHOP,
            "Country",
            text(MetadataField::Country).map(PropertyValue::Text),
        ),
        (
            PHOTOSHOP,
            "State",
            text(MetadataField::Region).map(PropertyValue::Text),
        ),
        (
            PHOTOSHOP,
            "City",
            text(MetadataField::City).map(PropertyValue::Text),
        ),
        (
            IPTC_CORE,
            "Location",
            text(MetadataField::Location).map(PropertyValue::Text),
        ),
    ];
    entries.extend(keyword_entries(content));
    entries.extend(gps_entries(content.gps));
    entries.push((
        TIFF,
        "Orientation",
        content
            .orientation
            .map(|orientation| PropertyValue::Text(orientation.get().to_string())),
    ));
    if let Some(date) = &content.capture_time {
        entries.extend(
            DATE_PROPERTIES.map(|(namespace, name)| {
                (namespace, name, Some(PropertyValue::Text(date.clone())))
            }),
        );
    }
    entries.push((
        PHOTOSHOP,
        "SidecarForExtension",
        content
            .sidecar_for_extension
            .clone()
            .map(PropertyValue::Text),
    ));
    entries.push((
        XMP,
        "MetadataDate",
        Some(PropertyValue::Text(content.metadata_date.clone())),
    ));
    entries
        .into_iter()
        .filter_map(|(namespace, name, value)| Some(property(namespace, name, value?)))
        .collect()
}

/// Mots-clés : la liste simple, puis les chemins de Lightroom.
fn keyword_entries(content: &XmpContent) -> [Entry; 2] {
    [
        (
            DC,
            "subject",
            (!content.subjects.is_empty()).then(|| PropertyValue::Bag(content.subjects.clone())),
        ),
        (
            LIGHTROOM,
            "hierarchicalSubject",
            (!content.keyword_paths.is_empty()).then(|| {
                PropertyValue::Bag(
                    content
                        .keyword_paths
                        .iter()
                        .map(|path| path.join(&LEVEL_SEPARATOR.to_string()))
                        .collect(),
                )
            }),
        ),
    ]
}

/// Position GPS, et altitude si elle est connue.
fn gps_entries(gps: Option<GpsPosition>) -> Vec<Entry> {
    let Some(gps) = gps else {
        return Vec::new();
    };
    let mut entries = vec![
        (
            EXIF,
            "GPSLatitude",
            Some(PropertyValue::Text(coordinate(gps.latitude, 'N', 'S'))),
        ),
        (
            EXIF,
            "GPSLongitude",
            Some(PropertyValue::Text(coordinate(gps.longitude, 'E', 'W'))),
        ),
    ];
    if let Some(altitude) = gps.altitude.filter(|altitude| altitude.is_finite()) {
        let reference = if altitude < 0.0 { "1" } else { "0" };
        entries.push((
            EXIF,
            "GPSAltitude",
            Some(PropertyValue::Text(altitude_rational(altitude))),
        ));
        entries.push((
            EXIF,
            "GPSAltitudeRef",
            Some(PropertyValue::Text(reference.to_owned())),
        ));
    }
    entries
}

fn property(namespace: &'static str, name: &'static str, value: PropertyValue) -> Property {
    Property {
        namespace,
        name,
        value,
    }
}

/// Nom de l'étiquette de couleur, comme l'écrit Lightroom avec son jeu
/// d'étiquettes par défaut.
fn label_name(label: ColorLabel) -> &'static str {
    match label {
        ColorLabel::Red => "Red",
        ColorLabel::Yellow => "Yellow",
        ColorLabel::Green => "Green",
        ColorLabel::Blue => "Blue",
        ColorLabel::Purple => "Purple",
    }
}

fn pick_name(flag: Flag) -> &'static str {
    match flag {
        Flag::Picked => "picked",
        Flag::Rejected => "rejected",
    }
}

/// Coordonnée en degrés et minutes décimales (« 51,44.510598N »), la forme
/// écrite par Lightroom ; six décimales : une dizaine de centimètres.
fn coordinate(degrees: f64, positive: char, negative: char) -> String {
    let direction = if degrees < 0.0 { negative } else { positive };
    let degrees = degrees.abs();
    let whole = degrees.trunc();
    let minutes = (degrees - whole) * 60.0;
    format!("{whole:.0},{minutes:.6}{direction}")
}

/// Altitude en fraction de centimètres, sans signe (le signe est dans
/// `GPSAltitudeRef`).
fn altitude_rational(altitude: f64) -> String {
    // Conversion sûre : l'altitude d'une photo tient largement dans un i64.
    #[allow(clippy::cast_possible_truncation)]
    let centimeters = (altitude.abs() * 100.0).round() as i64;
    format!("{centimeters}/100")
}
