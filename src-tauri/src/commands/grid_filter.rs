//! Filtres et tri de la grille, tels qu'échangés avec l'interface : filtres
//! Texte, Attribut et Métadonnées, tri.

use serde::{Deserialize, Serialize};

use super::map::MapAreaDto;
use specta::Type;

use crate::catalog::repositories::library::{
    ColorChoice, ColumnChoice, ColumnValue, FlagChoice, KindChoice, LibraryFilter, LibrarySort,
    MetadataColumn, RatingComparison, RatingRule, SortKey, TextSearch,
};
use crate::domain::{ColorLabel, Rating, TextField, TextRule};

/// Marqueur retenu par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridFlagChoice {
    /// Retenue.
    Picked,
    /// Sans marqueur.
    Unflagged,
    /// Rejetée.
    Rejected,
}

/// Comparaison de la note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridRatingComparison {
    /// =
    Equal,
    /// ≥
    AtLeast,
    /// ≤
    AtMost,
}

/// Règle de note du filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridRatingRule {
    /// Comparaison.
    pub comparison: GridRatingComparison,
    /// Nombre d'étoiles, ramené à 5 au plus.
    pub stars: u8,
}

/// Étiquette de couleur retenue par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridColorChoice {
    /// Rouge.
    Red,
    /// Jaune.
    Yellow,
    /// Vert.
    Green,
    /// Bleu.
    Blue,
    /// Violet.
    Purple,
    /// Sans couleur.
    None,
}

/// Type de fichier retenu par le filtre Attribut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridKindChoice {
    /// Photo (RAW compris).
    Photo,
    /// Vidéo.
    Video,
    /// Photo RAW.
    Raw,
    /// Photo d'une pile.
    Stack,
}

/// Champ cherché par le filtre Texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridTextField {
    /// Tous les champs, nom de fichier compris.
    All,
    /// Nom de fichier.
    FileName,
    /// Titre.
    Title,
    /// Légende.
    Caption,
    /// Mots-clés et leurs parents.
    Keywords,
    /// Pays, région, ville, lieu précis.
    Place,
}

/// Règle du filtre Texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridTextRule {
    /// Contient.
    Contains,
    /// Contient tout.
    ContainsAll,
    /// Contient les mots.
    ContainsWords,
    /// Ne contient pas.
    DoesNotContain,
    /// Commence par.
    StartsWith,
    /// Se termine par.
    EndsWith,
}

/// Filtre Texte de la grille.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridTextSearch {
    /// Champ cherché.
    pub field: GridTextField,
    /// Règle.
    pub rule: GridTextRule,
    /// Termes saisis (`portrait, !enfants`).
    pub text: String,
}

impl From<&GridTextSearch> for TextSearch {
    fn from(search: &GridTextSearch) -> Self {
        Self {
            field: match search.field {
                GridTextField::All => TextField::All,
                GridTextField::FileName => TextField::FileName,
                GridTextField::Title => TextField::Title,
                GridTextField::Caption => TextField::Caption,
                GridTextField::Keywords => TextField::Keywords,
                GridTextField::Place => TextField::Place,
            },
            rule: match search.rule {
                GridTextRule::Contains => TextRule::Contains,
                GridTextRule::ContainsAll => TextRule::ContainsAll,
                GridTextRule::ContainsWords => TextRule::ContainsWords,
                GridTextRule::DoesNotContain => TextRule::DoesNotContain,
                GridTextRule::StartsWith => TextRule::StartsWith,
                GridTextRule::EndsWith => TextRule::EndsWith,
            },
            text: search.text.clone(),
        }
    }
}

/// Critère d'une colonne du filtre Métadonnées.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridMetadataColumn {
    /// Date de prise de vue (année, mois, jour).
    Date,
    /// Appareil.
    Camera,
    /// Objectif.
    Lens,
    /// Étiquette de couleur.
    ColorLabel,
    /// Mot-clé.
    Keyword,
    /// Pays.
    Country,
    /// Région.
    Region,
    /// Ville.
    City,
    /// Lieu précis.
    Location,
    /// Type de fichier.
    FileType,
    /// ISO.
    Iso,
    /// Ouverture.
    Aperture,
    /// Vitesse.
    ShutterSpeed,
    /// Focale.
    FocalLength,
    /// Flash.
    Flash,
    /// Auteur.
    Creator,
    /// Dimensions.
    Dimensions,
    /// Orientation.
    Orientation,
}

impl From<GridMetadataColumn> for MetadataColumn {
    fn from(column: GridMetadataColumn) -> Self {
        match column {
            GridMetadataColumn::Date => Self::Date,
            GridMetadataColumn::Camera => Self::Camera,
            GridMetadataColumn::Lens => Self::Lens,
            GridMetadataColumn::ColorLabel => Self::ColorLabel,
            GridMetadataColumn::Keyword => Self::Keyword,
            GridMetadataColumn::Country => Self::Country,
            GridMetadataColumn::Region => Self::Region,
            GridMetadataColumn::City => Self::City,
            GridMetadataColumn::Location => Self::Location,
            GridMetadataColumn::FileType => Self::FileType,
            GridMetadataColumn::Iso => Self::Iso,
            GridMetadataColumn::Aperture => Self::Aperture,
            GridMetadataColumn::ShutterSpeed => Self::ShutterSpeed,
            GridMetadataColumn::FocalLength => Self::FocalLength,
            GridMetadataColumn::Flash => Self::Flash,
            GridMetadataColumn::Creator => Self::Creator,
            GridMetadataColumn::Dimensions => Self::Dimensions,
            GridMetadataColumn::Orientation => Self::Orientation,
        }
    }
}

/// Valeurs choisies dans une colonne du filtre Métadonnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridColumnChoice {
    /// Critère de la colonne.
    pub column: GridMetadataColumn,
    /// Clés des valeurs choisies ; `null` : « Inconnu ».
    pub values: Vec<Option<String>>,
}

/// Une valeur d'une colonne du filtre Métadonnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridColumnValue {
    /// Clé de la valeur (« 2024-07-14 », « 2.8 », « red ») ; `null` :
    /// « Inconnu ».
    pub value: Option<String>,
    /// Nom à afficher quand la clé n'en est pas un (nom d'un mot-clé).
    pub label: Option<String>,
    /// Précision (parent d'un mot-clé).
    pub detail: Option<String>,
    /// Nombre de médias.
    #[specta(type = u32)]
    pub count: u64,
}

impl From<ColumnValue> for GridColumnValue {
    fn from(value: ColumnValue) -> Self {
        Self {
            value: value.value,
            label: value.label,
            detail: value.detail,
            count: value.count,
        }
    }
}

/// Filtres Texte, Attribut et Métadonnées de la grille :
/// un critère vide ne retient rien de plus.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridFilter {
    /// Marqueurs retenus.
    pub flags: Vec<GridFlagChoice>,
    /// Règle de note.
    pub rating: Option<GridRatingRule>,
    /// Étiquettes de couleur retenues.
    pub colors: Vec<GridColorChoice>,
    /// Types de fichiers retenus.
    pub kinds: Vec<GridKindChoice>,
    /// Filtre Texte.
    pub text: Option<GridTextSearch>,
    /// Choix du filtre Métadonnées, colonne par colonne.
    pub metadata: Vec<GridColumnChoice>,
    /// Zone de la carte(« Limiter à la zone visible ») ; absente
    /// des préréglages, qui ne l'enregistrent pas.
    #[serde(default)]
    pub area: Option<MapAreaDto>,
}

impl From<&GridFilter> for LibraryFilter {
    fn from(filter: &GridFilter) -> Self {
        Self {
            flags: filter
                .flags
                .iter()
                .map(|flag| match flag {
                    GridFlagChoice::Picked => FlagChoice::Picked,
                    GridFlagChoice::Unflagged => FlagChoice::Unflagged,
                    GridFlagChoice::Rejected => FlagChoice::Rejected,
                })
                .collect(),
            rating: filter.rating.map(|rule| RatingRule {
                comparison: match rule.comparison {
                    GridRatingComparison::Equal => RatingComparison::Equal,
                    GridRatingComparison::AtLeast => RatingComparison::AtLeast,
                    GridRatingComparison::AtMost => RatingComparison::AtMost,
                },
                stars: rule.stars.min(Rating::MAX),
            }),
            colors: filter
                .colors
                .iter()
                .map(|color| match color {
                    GridColorChoice::Red => ColorChoice::Label(ColorLabel::Red),
                    GridColorChoice::Yellow => ColorChoice::Label(ColorLabel::Yellow),
                    GridColorChoice::Green => ColorChoice::Label(ColorLabel::Green),
                    GridColorChoice::Blue => ColorChoice::Label(ColorLabel::Blue),
                    GridColorChoice::Purple => ColorChoice::Label(ColorLabel::Purple),
                    GridColorChoice::None => ColorChoice::Unlabeled,
                })
                .collect(),
            kinds: filter
                .kinds
                .iter()
                .map(|kind| match kind {
                    GridKindChoice::Photo => KindChoice::Photo,
                    GridKindChoice::Video => KindChoice::Video,
                    GridKindChoice::Raw => KindChoice::Raw,
                    GridKindChoice::Stack => KindChoice::Stack,
                })
                .collect(),
            text: filter.text.as_ref().map(TextSearch::from),
            metadata: filter
                .metadata
                .iter()
                .map(|choice| ColumnChoice {
                    column: choice.column.into(),
                    values: choice.values.clone(),
                })
                .collect(),
            area: filter.area.and_then(MapAreaDto::area),
        }
    }
}

/// Critère de tri de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GridSortKey {
    /// Date de prise de vue.
    CaptureTime,
    /// Date d'import.
    ImportTime,
    /// Nom de fichier.
    FileName,
    /// Note.
    Rating,
    /// Marqueur.
    Flag,
    /// Étiquette de couleur.
    ColorLabel,
    /// Type de fichier.
    FileType,
}

/// Tri de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridSort {
    /// Critère.
    pub key: GridSortKey,
    /// Ordre décroissant.
    pub descending: bool,
}

impl From<GridSort> for LibrarySort {
    fn from(sort: GridSort) -> Self {
        Self {
            key: match sort.key {
                GridSortKey::CaptureTime => SortKey::CaptureTime,
                GridSortKey::ImportTime => SortKey::ImportTime,
                GridSortKey::FileName => SortKey::FileName,
                GridSortKey::Rating => SortKey::Rating,
                GridSortKey::Flag => SortKey::Flag,
                GridSortKey::ColorLabel => SortKey::ColorLabel,
                GridSortKey::FileType => SortKey::FileType,
            },
            descending: sort.descending,
        }
    }
}
