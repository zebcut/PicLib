//! Règles des collections dynamiques : des règles
//! combinables (toutes, ou au moins une), en groupes imbriqués, traduites
//! en une condition SQL sur `media`, évaluée à chaque lecture : le contenu
//! est toujours à jour.
//!
//! Les règles sont rangées en JSON dans le catalogue et échangées telles
//! quelles avec l'interface (d'où `Serialize`, `Deserialize` et `Type`) ;
//! `smart_sql` les traduit en SQL.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Combinaison des règles d'un groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartCombine {
    /// Toutes les règles.
    All,
    /// Au moins une des règles.
    Any,
}

/// Comparaison d'un nombre d'étoiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartRatingComparison {
    /// ≥
    AtLeast,
    /// =
    Equal,
    /// ≤
    AtMost,
}

/// Comparaison d'une date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartDateComparison {
    /// Avant ce jour.
    Before,
    /// Ce jour-là.
    On,
    /// Après ce jour.
    After,
}

/// Marqueur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartFlag {
    /// Retenue.
    Picked,
    /// Sans marqueur.
    Unflagged,
    /// Rejetée.
    Rejected,
}

/// Étiquette de couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartColor {
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

/// Type de fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartKind {
    /// Photo (RAW compris).
    Photo,
    /// Vidéo.
    Video,
    /// Photo RAW.
    Raw,
}

/// Champ d'une règle de texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartTextField {
    /// Nom de fichier.
    FileName,
    /// Titre.
    Title,
    /// Légende.
    Caption,
    /// Pays, région, ville, lieu précis.
    Place,
    /// Names of the applied keywords and of their parents, whether the
    /// keyword exists yet or not ("France" matches "Places > France > Paris").
    Keywords,
}

/// Règle d'une collection dynamique, ou groupe de règles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SmartRule {
    /// Groupe : toutes ses règles, ou au moins une.
    Group {
        /// Combinaison.
        combine: SmartCombine,
        /// Règles et sous-groupes.
        rules: Vec<SmartRule>,
    },
    /// Note.
    Rating {
        /// Comparaison.
        comparison: SmartRatingComparison,
        /// Étoiles, de 0 à 5.
        stars: u8,
    },
    /// Marqueur.
    Flag {
        /// Marqueur.
        flag: SmartFlag,
    },
    /// Étiquette de couleur.
    Color {
        /// Couleur.
        color: SmartColor,
    },
    /// Mot-clé posé.
    #[serde(rename_all = "camelCase")]
    Keyword {
        /// Mot-clé.
        keyword_id: u32,
        /// Ses sous-mots-clés aussi(coché par défaut).
        include_children: bool,
    },
    /// Date de prise de vue.
    CaptureDate {
        /// Comparaison.
        comparison: SmartDateComparison,
        /// Jour (`2024-07-14`).
        date: String,
    },
    /// Date d'import.
    ImportDate {
        /// Comparaison.
        comparison: SmartDateComparison,
        /// Jour (`2024-07-14`).
        date: String,
    },
    /// Appareil : le modèle ou la marque contient ce texte.
    Camera {
        /// Texte cherché.
        text: String,
    },
    /// Objectif : son nom contient ce texte.
    Lens {
        /// Texte cherché.
        text: String,
    },
    /// Type de fichier.
    #[serde(rename_all = "camelCase")]
    FileKind {
        /// Type.
        file_kind: SmartKind,
    },
    /// Dossier.
    #[serde(rename_all = "camelCase")]
    Folder {
        /// Dossier.
        folder_id: u32,
        /// Ses sous-dossiers aussi.
        include_subfolders: bool,
    },
    /// Texte, comme le filtre Texte (sans majuscules ni accents).
    Text {
        /// Champ cherché.
        field: SmartTextField,
        /// Règle.
        rule: SmartTextRule,
        /// Termes saisis.
        text: String,
    },
    /// Position GPS présente (ou absente).
    Gps {
        /// Présente.
        present: bool,
    },
    /// Appartenance à une pile (d'au moins deux photos).
    Stack {
        /// Dans une pile ; faux : hors de toute pile.
        stacked: bool,
    },
}

/// Règle de comparaison d'une règle de texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SmartTextRule {
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

/// Règles enregistrées, lues ; `None` si le texte est illisible
/// (journalisé).
#[must_use]
pub fn parse(rules: &str) -> Option<SmartRule> {
    serde_json::from_str(rules)
        .inspect_err(|error| tracing::warn!(%error, "unreadable smart collection rules"))
        .ok()
}
