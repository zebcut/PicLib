//! Attributs de tri d'une photo : marqueur, note, étiquette de
//! couleur.

/// Marqueur d'une photo ; l'absence de marqueur est `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flag {
    /// Retenue (P).
    Picked,
    /// Rejetée (X).
    Rejected,
}

/// Note de 0 (sans note) à 5 étoiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Rating(u8);

impl Rating {
    /// Note la plus haute.
    pub const MAX: u8 = 5;

    /// Note valide (0 à 5), sinon `None`.
    #[must_use]
    pub fn new(stars: u8) -> Option<Self> {
        (stars <= Self::MAX).then_some(Self(stars))
    }

    /// Nombre d'étoiles.
    #[must_use]
    pub fn stars(self) -> u8 {
        self.0
    }
}

/// Étiquette de couleur, fixe comme dans Lightroom ; une seule par photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorLabel {
    /// Rouge (6).
    Red,
    /// Jaune (7).
    Yellow,
    /// Vert (8).
    Green,
    /// Bleu (9).
    Blue,
    /// Violet (menu seulement).
    Purple,
}
