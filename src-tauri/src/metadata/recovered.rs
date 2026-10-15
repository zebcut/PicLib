//! Métadonnées déjà posées par d'autres logiciels (note, couleur, titre,
//! mots-clés...), récupérées à l'import, et fusion des sources
//! par ordre de priorité.

use std::collections::BTreeSet;

use crate::domain::{
    CaptureTime, ColorLabel, Flag, GpsPosition, MAX_METADATA_CHARS, MetadataField, Orientation,
    Rating, Rotation, keyword_name_key, metadata_text,
};

/// Écart en degrés au-delà duquel deux positions GPS diffèrent (environ un
/// mètre) : les logiciels n'arrondissent pas tous de la même façon.
const GPS_TOLERANCE_DEGREES: f64 = 0.000_01;

/// Chemin d'un mot-clé trouvé dans un fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordPath {
    /// Chemin complet depuis la racine (champ hiérarchique de Lightroom,
    /// `Lieux|France|Paris`), du plus haut niveau au mot-clé.
    FromRoot(Vec<String>),
    /// Nom seul (liste simple `dc:subject`).
    Name(String),
}

impl KeywordPath {
    /// Noms de tous les niveaux du chemin.
    fn names(&self) -> &[String] {
        match self {
            Self::FromRoot(path) => path,
            Self::Name(name) => std::slice::from_ref(name),
        }
    }
}

/// Métadonnées trouvées dans une source (EXIF, XMP intégré, XMP annexe).
/// Chaque information est facultative : une source n'en donne souvent
/// qu'une partie.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecoveredMetadata {
    /// Note de 1 à 5 étoiles (0 étoile, écrit par tous les appareils, ne
    /// dit rien).
    pub rating: Option<Rating>,
    /// Marqueur (seul « rejetée » s'écrit dans les fichiers : note -1).
    pub flag: Option<Flag>,
    /// Étiquette de couleur.
    pub color_label: Option<ColorLabel>,
    /// Champs descriptifs non vides, chacun au plus une fois.
    pub fields: Vec<(MetadataField, String)>,
    /// Position GPS.
    pub gps: Option<GpsPosition>,
    /// Orientation de l'image à l'affichage.
    pub orientation: Option<Orientation>,
    /// Date de prise de vue (utilisée seulement si l'EXIF n'en a pas).
    pub capture_time: Option<CaptureTime>,
    /// Mots-clés.
    pub keywords: Vec<KeywordPath>,
}

impl RecoveredMetadata {
    /// Valeur d'un champ descriptif.
    #[must_use]
    pub fn field(&self, field: MetadataField) -> Option<&str> {
        self.fields
            .iter()
            .find(|(other, _)| *other == field)
            .map(|(_, value)| value.as_str())
    }

    /// Noms de tous les niveaux des mots-clés, sans majuscules : deux
    /// sources qui écrivent les mêmes mots-clés, l'une avec leur chemin,
    /// l'autre en liste simple avec les parents, ne sont pas en conflit.
    fn keyword_names(&self) -> BTreeSet<String> {
        self.keywords
            .iter()
            .flat_map(KeywordPath::names)
            .map(|name| keyword_name_key(name))
            .collect()
    }
}

/// Résultat de la fusion des sources d'un fichier.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recovery {
    /// Pour chaque information, celle de la source la plus prioritaire qui
    /// la donne.
    pub metadata: RecoveredMetadata,
    /// Vrai si deux sources donnent des valeurs différentes pour une même
    /// information (la plus prioritaire l'emporte).
    pub has_conflict: bool,
}

/// Fusionne les sources, de la plus prioritaire à la moins prioritaire
/// (XMP annexe, puis métadonnées intégrées). Chaque information
/// vient de la première source qui la donne ; les mots-clés viennent tous de
/// la première source qui en a (sans mélange : un mot-clé retiré dans
/// Lightroom ne revient pas par l'XMP intégré, plus ancien).
///
/// L'orientation et la date ne comptent pas dans les conflits : une
/// orientation différente est une rotation faite dans l'autre logiciel, et
/// la date de l'XMP ne sert qu'en l'absence de date EXIF.
#[must_use]
pub fn merge(sources: &[RecoveredMetadata]) -> Recovery {
    let mut has_conflict = false;
    let rating = first(
        sources.iter().map(|s| s.rating),
        PartialEq::eq,
        &mut has_conflict,
    );
    let flag = first(
        sources.iter().map(|s| s.flag),
        PartialEq::eq,
        &mut has_conflict,
    );
    let color_label = first(
        sources.iter().map(|s| s.color_label),
        PartialEq::eq,
        &mut has_conflict,
    );
    let gps = first(
        sources.iter().map(|s| s.gps),
        same_position,
        &mut has_conflict,
    );
    let fields = MetadataField::ALL
        .into_iter()
        .filter_map(|field| {
            let value = first(
                sources.iter().map(|s| s.field(field)),
                PartialEq::eq,
                &mut has_conflict,
            )?;
            Some((field, value.to_owned()))
        })
        .collect();
    let keywords = first(
        sources.iter().map(|source| {
            let names = source.keyword_names();
            (!names.is_empty()).then_some((names, &source.keywords))
        }),
        |(a, _), (b, _)| a == b,
        &mut has_conflict,
    );

    Recovery {
        metadata: RecoveredMetadata {
            rating,
            flag,
            color_label,
            fields,
            gps,
            orientation: sources.iter().find_map(|source| source.orientation),
            capture_time: sources.iter().find_map(|source| source.capture_time),
            keywords: keywords.map_or_else(Vec::new, |(_, paths)| paths.clone()),
        },
        has_conflict,
    }
}

/// Première valeur présente ; `has_conflict` devient vrai si une valeur
/// suivante en diffère.
fn first<T>(
    values: impl IntoIterator<Item = Option<T>>,
    same: impl Fn(&T, &T) -> bool,
    has_conflict: &mut bool,
) -> Option<T> {
    let mut present = values.into_iter().flatten();
    let first = present.next()?;
    *has_conflict |= present.any(|other| !same(&first, &other));
    Some(first)
}

fn same_position(a: &GpsPosition, b: &GpsPosition) -> bool {
    (a.latitude - b.latitude).abs() <= GPS_TOLERANCE_DEGREES
        && (a.longitude - b.longitude).abs() <= GPS_TOLERANCE_DEGREES
}

/// Valeur enregistrable d'un champ : non vide, et pas démesurée (un texte
/// trop long est ignoré plutôt que coupé au milieu).
#[must_use]
pub fn field_value(field: MetadataField, text: &str) -> Option<String> {
    let value = metadata_text(text)?;
    if value.chars().count() > MAX_METADATA_CHARS {
        tracing::warn!(?field, "recovered value too long, ignored");
        return None;
    }
    Some(value)
}

/// Rotation à appliquer dans PicLib pour afficher l'image comme dans
/// l'autre logiciel : l'écart entre l'orientation qu'il a enregistrée et
/// celle du fichier (normale si elle est absente). Une orientation en
/// miroir n'est pas une rotation : elle est ignorée.
#[must_use]
pub fn rotation_between(file: Option<Orientation>, recovered: Option<Orientation>) -> Rotation {
    let angle = |orientation: Orientation| match orientation.get() {
        1 => Some(0),
        6 => Some(90),
        3 => Some(180),
        8 => Some(270),
        _ => None,
    };
    let Some(recovered) = recovered.and_then(angle) else {
        return Rotation::default();
    };
    let file = match file {
        Some(file) => angle(file),
        None => Some(0),
    };
    file.and_then(|file| Rotation::from_degrees((recovered + 360 - file) % 360))
        .unwrap_or_default()
}
