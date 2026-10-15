//! Lieu d'une photo retrouvé à partir de sa position GPS.

/// Langue des textes que PicLib écrit lui-même dans le catalogue (nom d'un
/// pays...) : celle de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Français.
    French,
    /// Anglais.
    English,
}

/// Noms du lieu le plus proche d'une position GPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceNames {
    /// Pays, dans la langue demandée.
    pub country: String,
    /// Région (état, province...), si elle est connue.
    pub region: Option<String>,
    /// Ville ou village.
    pub city: String,
}
