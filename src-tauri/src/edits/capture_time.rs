//! Modification annulable de la date de prise de vue.
//!
//! Seulement ce que l'utilisateur demande : une date donnée telle quelle à
//! toutes les photos sélectionnées, ou le retour à leur date d'origine.
//! Aucun décalage ni calcul automatique ; le décalage par rapport à UTC
//! n'est pas touché par une date saisie.

use jiff::civil::DateTime;
use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::capture_time::{self, StoredCaptureTime};
use crate::domain::{CaptureTime, MediaId};

/// Années possibles d'une date de prise de vue : le format de stockage les
/// écrit sur quatre chiffres.
const YEARS: std::ops::RangeInclusive<i16> = 1..=9999;

/// Changement de date demandé par l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTimeChange {
    /// Donner cette date locale à tous les médias.
    Set(DateTime),
    /// Revenir à la date lue à l'import.
    Original,
}

/// Raison pour laquelle le changement n'a pas pu être préparé.
#[derive(Debug, thiserror::Error)]
pub enum CaptureTimeError {
    /// Le catalogue n'a pas pu être lu.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// Date hors de l'an 1 à l'an 9999.
    #[error("the capture time is out of range")]
    OutOfRange,
}

/// Ancienne et nouvelle date d'un média.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DateChange {
    media_id: MediaId,
    before: StoredCaptureTime,
    after: StoredCaptureTime,
}

/// Nouvelle date de prise de vue pour plusieurs médias. Au retour à la date
/// d'origine, chaque média retrouve la sienne : les dates sont donc toutes
/// retenues (environ 100 octets par média).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTimeEdit {
    /// Médias dont la date change réellement.
    changes: Vec<DateChange>,
}

impl CaptureTimeEdit {
    /// Prépare le changement : lit les dates actuelles des médias `ids` (les
    /// identifiants inconnus sont ignorés). Les médias dont la date ne
    /// change pas sont laissés de côté.
    ///
    /// # Errors
    ///
    /// Voir [`CaptureTimeError`] ; rien n'est modifié.
    pub(super) fn new(
        connection: &Connection,
        change: CaptureTimeChange,
        ids: &[MediaId],
    ) -> Result<Self, CaptureTimeError> {
        let typed = match change {
            CaptureTimeChange::Set(local) => {
                if !YEARS.contains(&local.year()) {
                    return Err(CaptureTimeError::OutOfRange);
                }
                Some(CaptureTime::local_storage_text(local))
            }
            CaptureTimeChange::Original => None,
        };
        let mut changes = Vec::new();
        for (media_id, times) in capture_time::read(connection, ids)? {
            let after = if let Some(local) = &typed {
                StoredCaptureTime {
                    local: local.clone(),
                    offset_seconds: times.current.offset_seconds,
                }
            } else {
                // Une date d'origine illisible n'est jamais recopiée : la
                // date actuelle deviendrait illisible à son tour.
                CaptureTime::parse_storage_text(&times.original.local).map_err(|_| {
                    CatalogError::InvalidValue {
                        column: "original_capture_time",
                        value: times.original.local.clone(),
                    }
                })?;
                times.original
            };
            if after != times.current {
                changes.push(DateChange {
                    media_id,
                    before: times.current,
                    after,
                });
            }
        }
        Ok(Self { changes })
    }

    /// Médias dont la date change.
    #[must_use]
    pub fn media_ids(&self) -> Vec<MediaId> {
        self.changes.iter().map(|change| change.media_id).collect()
    }

    /// Vrai si aucune date ne change.
    pub(super) fn changes_nothing(&self) -> bool {
        self.changes.is_empty()
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for change in &self.changes {
            capture_time::write(connection, change.media_id, &change.after)?;
        }
        Ok(())
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        for change in &self.changes {
            capture_time::write(connection, change.media_id, &change.before)?;
        }
        Ok(())
    }
}
