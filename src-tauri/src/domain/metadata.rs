//! Métadonnées descriptives modifiables d'une photo.

/// Champ de métadonnées descriptives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataField {
    /// Titre.
    Title,
    /// Légende.
    Caption,
    /// Auteur.
    Creator,
    /// Copyright.
    Copyright,
    /// Pays.
    Country,
    /// Région.
    Region,
    /// Ville.
    City,
    /// Lieu précis.
    Location,
}

impl MetadataField {
    /// Tous les champs, dans l'ordre du panneau Métadonnées.
    pub const ALL: [Self; 8] = [
        Self::Title,
        Self::Caption,
        Self::Creator,
        Self::Copyright,
        Self::Country,
        Self::Region,
        Self::City,
        Self::Location,
    ];
}

/// Longueur maximale d'un champ, en caractères : bien au-delà d'une légende
/// ordinaire, mais évite d'enregistrer par erreur un texte énorme collé.
pub const MAX_METADATA_CHARS: usize = 10_000;

/// Valeur enregistrée pour un texte saisi : sans espaces au début ni à la
/// fin, et `None` pour un texte vide (champ effacé).
#[must_use]
pub fn metadata_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
