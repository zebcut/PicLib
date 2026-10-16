//! Métadonnées des vidéos (date de création, durée, dimensions, GPS).

use std::fs::File;
use std::path::Path;

use jiff::tz::TimeZone;
use nom_exif::{MediaParser, MediaSource, TrackInfoTag};

use super::values::{gps_position, pixel_size, text, unsigned, video_capture_time};
use super::{ExtractedMetadata, MetadataError, RecoveredMetadata};
use crate::domain::MediaDetails;

/// Lit les informations d'une vidéo. Un conteneur que la bibliothèque ne
/// sait pas lire (AVI, MTS) ou un contenu non reconnu donne une vidéo sans
/// métadonnées, comme pour les photos : ce n'est pas une erreur.
pub fn read(path: &Path) -> Result<ExtractedMetadata, MetadataError> {
    let parsed = MediaSource::seekable(File::open(path)?)
        .and_then(|source| MediaParser::new().parse_track(source));
    let track = match parsed {
        Ok(track) => track,
        Err(nom_exif::Error::TrackNotFound | nom_exif::Error::UnsupportedFormat) => {
            return Ok(ExtractedMetadata::default());
        }
        Err(error) => return Err(to_error(error)),
    };

    let pixel = |tag| pixel_size(track.get(tag));
    let gps = track.gps_info().and_then(gps_position);
    Ok(ExtractedMetadata {
        capture_time: video_capture_time(track.get(TrackInfoTag::CreateDate), &TimeZone::system()),
        details: MediaDetails {
            width: pixel(TrackInfoTag::Width),
            height: pixel(TrackInfoTag::Height),
            camera_make: text(track.get(TrackInfoTag::Make)),
            camera_model: text(track.get(TrackInfoTag::Model)),
            gps,
            duration_ms: unsigned(track.get(TrackInfoTag::DurationMs)),
            ..MediaDetails::default()
        },
        recovered: RecoveredMetadata {
            gps,
            ..RecoveredMetadata::default()
        },
    })
}

fn to_error(error: nom_exif::Error) -> MetadataError {
    match error {
        nom_exif::Error::Io(error) => MetadataError::Io(error),
        other => MetadataError::Unreadable(other.to_string()),
    }
}
