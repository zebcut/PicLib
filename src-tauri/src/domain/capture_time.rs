//! Date de prise de vue.

use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};

use super::Timestamp;

/// Format de stockage et de tri d'une date locale : `2024-07-14T18:30:05`.
/// L'ordre alphabétique de ce texte est l'ordre chronologique.
const STORAGE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// Origine de la date de prise de vue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureTimeSource {
    /// Métadonnées du fichier : EXIF (ou date de création d'une vidéo), ou
    /// XMP à défaut d'EXIF.
    Exif,
    /// Date de modification du fichier, faute de mieux.
    FileModified,
}

/// Date et heure de prise de vue, telles qu'affichées par l'appareil.
///
/// L'EXIF donne souvent une heure **locale sans fuseau** : on la conserve
/// telle quelle (`local`), avec son décalage par rapport à UTC quand il est
/// connu (`offset`). On ne la convertit jamais en UTC, pour ne pas décaler
/// les photos prises en voyage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureTime {
    /// Date et heure locales, à la seconde.
    pub local: DateTime,
    /// Décalage par rapport à UTC, s'il est connu (`+02:00`...).
    pub offset: Option<Offset>,
    /// Origine de la date.
    pub source: CaptureTimeSource,
}

impl CaptureTime {
    /// Date lue dans les métadonnées, arrondie à la seconde.
    #[must_use]
    pub fn from_exif(local: DateTime, offset: Option<Offset>) -> Self {
        Self {
            local: truncate_to_second(local),
            offset,
            source: CaptureTimeSource::Exif,
        }
    }

    /// Date de repli : la date de modification du fichier, exprimée dans le
    /// fuseau horaire `time_zone` (celui du Mac, en pratique).
    #[must_use]
    pub fn from_file_modified(modified: Timestamp, time_zone: &TimeZone) -> Self {
        let instant = jiff::Timestamp::from_millisecond(modified.unix_millis())
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        Self {
            local: truncate_to_second(time_zone.to_datetime(instant)),
            offset: Some(time_zone.to_offset(instant)),
            source: CaptureTimeSource::FileModified,
        }
    }

    /// Texte de stockage et de tri de la date locale (`2024-07-14T18:30:05`).
    #[must_use]
    pub fn storage_text(&self) -> String {
        Self::local_storage_text(self.local)
    }

    /// Texte de stockage d'une date locale quelconque, à la seconde.
    #[must_use]
    pub fn local_storage_text(local: DateTime) -> String {
        truncate_to_second(local)
            .strftime(STORAGE_FORMAT)
            .to_string()
    }

    /// Relit une date locale écrite par [`CaptureTime::storage_text`].
    ///
    /// # Errors
    ///
    /// Échoue si le texte n'a pas le format attendu.
    pub fn parse_storage_text(text: &str) -> Result<DateTime, jiff::Error> {
        DateTime::strptime(STORAGE_FORMAT, text)
    }
}

fn truncate_to_second(datetime: DateTime) -> DateTime {
    datetime
        .with()
        .subsec_nanosecond(0)
        .build()
        .unwrap_or(datetime)
}
