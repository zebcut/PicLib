//! Instant précis, indépendant du fuseau horaire.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Instant exprimé en millisecondes depuis le 1er janvier 1970 à minuit UTC
/// (« temps Unix »).
///
/// Sert aux dates techniques (modification d'un fichier, import). La date de
/// prise de vue, qui n'a souvent pas de fuseau horaire dans l'EXIF, aura son
/// propre type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Crée un instant à partir d'un nombre de millisecondes depuis 1970.
    #[must_use]
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Nombre de millisecondes depuis 1970 (négatif avant 1970).
    #[must_use]
    pub const fn unix_millis(self) -> i64 {
        self.0
    }

    /// Instant présent selon l'horloge du système.
    #[must_use]
    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    /// Convertit un instant du système (par exemple la date de modification
    /// d'un fichier). Les valeurs hors de portée (au-delà de ± 292 millions
    /// d'années) sont ramenées à la limite la plus proche.
    #[must_use]
    pub fn from_system_time(time: SystemTime) -> Self {
        match time.duration_since(UNIX_EPOCH) {
            Ok(after) => Self(duration_to_millis(after)),
            Err(error) => Self(-duration_to_millis(error.duration())),
        }
    }
}

fn duration_to_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
