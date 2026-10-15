//! Des propriétés XMP aux métadonnées de PicLib : note, couleur, champs
//! descriptifs, GPS, orientation, date et mots-clés.

use super::namespaces::{DC, EXIF, IPTC_CORE, LIGHTROOM, PHOTOSHOP, PICLIB, TIFF, XMP};
use super::rdf::Properties;
use crate::domain::{
    ColorLabel, Flag, GpsPosition, MetadataField, Orientation, Rating, recovered_keyword_name,
};
use crate::metadata::recovered::{KeywordPath, RecoveredMetadata, field_value};
use crate::metadata::values::{is_plausible_position, parse_iso_date};

/// Séparateur des niveaux dans le champ hiérarchique de Lightroom.
pub(super) const LEVEL_SEPARATOR: char = '|';

/// Séparateur de plusieurs auteurs réunis dans un seul champ.
const CREATOR_SEPARATOR: &str = "; ";

/// Métadonnées d'un paquet XMP.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmpMetadata {
    /// Informations récupérées.
    pub metadata: RecoveredMetadata,
    /// Extension de la photo que décrit un fichier annexe partagé par un
    /// RAW et un JPEG (`photoshop:SidecarForExtension`, « NEF »).
    pub sidecar_for_extension: Option<String>,
}

/// Lit les informations utiles d'un paquet XMP ; une valeur illisible est
/// ignorée (le reste du paquet sert quand même).
pub fn extract(properties: &Properties) -> XmpMetadata {
    let (rating, rejected) = rating_and_flag(properties.text(XMP, "Rating"));
    // Le marqueur écrit par PicLib passe avant la note -1 d'Adobe Bridge.
    let flag = properties.text(PICLIB, "Pick").and_then(pick).or(rejected);
    XmpMetadata {
        metadata: RecoveredMetadata {
            rating,
            flag,
            color_label: properties.text(XMP, "Label").and_then(color_label),
            fields: descriptive_fields(properties),
            gps: gps_position(properties),
            orientation: properties
                .text(TIFF, "Orientation")
                .and_then(|text| text.parse().ok())
                .and_then(Orientation::new),
            capture_time: [
                (EXIF, "DateTimeOriginal"),
                (PHOTOSHOP, "DateCreated"),
                (XMP, "CreateDate"),
            ]
            .into_iter()
            .find_map(|(namespace, name)| {
                properties.text(namespace, name).and_then(parse_iso_date)
            }),
            keywords: keywords(properties),
        },
        sidecar_for_extension: properties
            .text(PHOTOSHOP, "SidecarForExtension")
            .map(str::to_owned),
    }
}

/// Note et marqueur : 1 à 5 étoiles ; -1 signifie « rejetée » (Adobe
/// Bridge) ; 0 ne dit rien (tous les appareils l'écrivent).
fn rating_and_flag(text: Option<&str>) -> (Option<Rating>, Option<Flag>) {
    let Some(value) = text.and_then(|text| text.parse::<f64>().ok()) else {
        return (None, None);
    };
    let value = value.round();
    if (value - -1.0).abs() < f64::EPSILON {
        return (None, Some(Flag::Rejected));
    }
    if !(1.0..=5.0).contains(&value) {
        return (None, None);
    }
    // Conversion sûre : `value` est un entier de 1 à 5.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let stars = value as u8;
    (Rating::new(stars), None)
}

/// Marqueur écrit par PicLib (`piclib:Pick`).
fn pick(text: &str) -> Option<Flag> {
    match text {
        "picked" => Some(Flag::Picked),
        "rejected" => Some(Flag::Rejected),
        _ => None,
    }
}

/// Étiquette de couleur : Lightroom écrit le nom de la couleur dans la
/// langue de son jeu d'étiquettes (anglais ou français ici).
fn color_label(name: &str) -> Option<ColorLabel> {
    let label = match name.to_lowercase().as_str() {
        "red" | "rouge" => ColorLabel::Red,
        "yellow" | "jaune" => ColorLabel::Yellow,
        "green" | "vert" => ColorLabel::Green,
        "blue" | "bleu" => ColorLabel::Blue,
        "purple" | "violet" => ColorLabel::Purple,
        _ => {
            tracing::debug!(label = name, "unknown color label ignored");
            return None;
        }
    };
    Some(label)
}

fn descriptive_fields(properties: &Properties) -> Vec<(MetadataField, String)> {
    let creators = properties.list(DC, "creator").join(CREATOR_SEPARATOR);
    [
        (MetadataField::Title, properties.text(DC, "title")),
        (MetadataField::Caption, properties.text(DC, "description")),
        (MetadataField::Creator, Some(creators.as_str())),
        (MetadataField::Copyright, properties.text(DC, "rights")),
        (
            MetadataField::Country,
            properties.text(PHOTOSHOP, "Country"),
        ),
        (MetadataField::Region, properties.text(PHOTOSHOP, "State")),
        (MetadataField::City, properties.text(PHOTOSHOP, "City")),
        (
            MetadataField::Location,
            properties.text(IPTC_CORE, "Location"),
        ),
    ]
    .into_iter()
    .filter_map(|(field, text)| Some((field, field_value(field, text?)?)))
    .collect()
}

/// Position GPS, écrite comme « 48,51.3N » (degrés, minutes décimales) ou
/// « 48,51,18N » (degrés, minutes, secondes).
fn gps_position(properties: &Properties) -> Option<GpsPosition> {
    let latitude = coordinate(properties.text(EXIF, "GPSLatitude")?, 'N', 'S')?;
    let longitude = coordinate(properties.text(EXIF, "GPSLongitude")?, 'E', 'W')?;
    if !is_plausible_position(latitude, longitude) {
        return None;
    }
    let below_sea_level = properties.text(EXIF, "GPSAltitudeRef") == Some("1");
    let altitude = properties
        .text(EXIF, "GPSAltitude")
        .and_then(rational)
        .map(|meters| if below_sea_level { -meters } else { meters });
    Some(GpsPosition {
        latitude,
        longitude,
        altitude,
    })
}

fn coordinate(text: &str, positive: char, negative: char) -> Option<f64> {
    let direction = text.chars().last()?.to_ascii_uppercase();
    let sign = match direction {
        d if d == positive => 1.0,
        d if d == negative => -1.0,
        _ => return None,
    };
    let numbers: Vec<f64> = text[..text.len() - 1]
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    let degrees = match numbers.as_slice() {
        [degrees, minutes] => degrees + minutes / 60.0,
        [degrees, minutes, seconds] => degrees + minutes / 60.0 + seconds / 3600.0,
        _ => return None,
    };
    degrees.is_finite().then_some(sign * degrees)
}

/// Nombre écrit en fraction (« 1234/10 ») ou en décimal.
fn rational(text: &str) -> Option<f64> {
    let value = match text.split_once('/') {
        Some((numerator, denominator)) => {
            numerator.trim().parse::<f64>().ok()? / denominator.trim().parse::<f64>().ok()?
        }
        None => text.trim().parse().ok()?,
    };
    value.is_finite().then_some(value)
}

/// Mots-clés : chemins du champ hiérarchique de Lightroom s'il existe,
/// sinon noms de la liste simple. Un chemin déjà contenu dans un chemin plus
/// long (`Lieux|France` à côté de `Lieux|France|Paris`) n'est pas repris :
/// dans PicLib, poser Paris suffit.
fn keywords(properties: &Properties) -> Vec<KeywordPath> {
    let paths: Vec<Vec<String>> = properties
        .list(LIGHTROOM, "hierarchicalSubject")
        .into_iter()
        .map(|path| {
            path.split(LEVEL_SEPARATOR)
                .filter_map(recovered_keyword_name)
                .collect::<Vec<_>>()
        })
        .filter(|path| !path.is_empty())
        .collect();
    if paths.is_empty() {
        let mut names: Vec<String> = Vec::new();
        for name in properties
            .list(DC, "subject")
            .into_iter()
            .filter_map(recovered_keyword_name)
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        return names.into_iter().map(KeywordPath::Name).collect();
    }

    let mut kept: Vec<Vec<String>> = Vec::new();
    for path in &paths {
        let is_covered = paths
            .iter()
            .any(|other| other.len() > path.len() && other.starts_with(path));
        if !is_covered && !kept.contains(path) {
            kept.push(path.clone());
        }
    }
    kept.into_iter().map(KeywordPath::FromRoot).collect()
}
