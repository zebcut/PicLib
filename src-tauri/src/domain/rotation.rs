//! Rotation d'une photo choisie par l'utilisateur : un nombre de
//! quarts de tour, appliqué à l'affichage seulement.

/// Rotation dans le sens horaire : 0, 90, 180 ou 270 degrés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rotation(u16);

/// Sens d'un quart de tour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RotationDirection {
    /// Sens inverse des aiguilles d'une montre (Cmd+[).
    Left,
    /// Sens des aiguilles d'une montre (Cmd+]).
    Right,
}

impl Rotation {
    /// Rotation valide (0, 90, 180 ou 270 degrés), sinon `None`.
    #[must_use]
    pub fn from_degrees(degrees: u16) -> Option<Self> {
        matches!(degrees, 0 | 90 | 180 | 270).then_some(Self(degrees))
    }

    /// Angle en degrés, dans le sens horaire.
    #[must_use]
    pub fn degrees(self) -> u16 {
        self.0
    }
}

impl RotationDirection {
    /// Quart de tour exprimé en degrés dans le sens horaire (270 pour la
    /// gauche : trois quarts de tour à droite).
    #[must_use]
    pub fn clockwise_degrees(self) -> u16 {
        match self {
            Self::Left => 270,
            Self::Right => 90,
        }
    }

    /// Sens contraire, pour annuler.
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}
