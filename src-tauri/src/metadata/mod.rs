//! Lecture des métadonnées des photos (EXIF, RAW compris, et XMP) et des
//! vidéos.
//!
//! **Lecture seule** : les fichiers sont ouverts en lecture, et seul leur
//! début est lu dans la plupart des cas. Les bibliothèques utilisées
//! (`nom-exif`, `quick-xml`) sont isolées ici : le reste de l'application ne
//! connaît que les types de [`crate::domain`].

pub mod dop;
pub mod embedded;
mod iptc;
mod photo;
pub mod recovered;
mod values;
mod video;
pub mod xmp;

use std::path::Path;

pub use recovered::{KeywordPath, RecoveredMetadata, Recovery, merge, rotation_between};

use crate::domain::{CaptureTime, MediaDetails, MediaFormat, MediaKind};

/// Métadonnées extraites d'un fichier.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedMetadata {
    /// Date de prise de vue lue dans le fichier, si elle y figure.
    pub capture_time: Option<CaptureTime>,
    /// Informations techniques.
    pub details: MediaDetails,
    /// Métadonnées descriptives lues dans l'EXIF (auteur, copyright, GPS),
    /// à fusionner avec celles du XMP.
    pub recovered: RecoveredMetadata,
}

/// Erreur de lecture des métadonnées.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// Le fichier n'a pas pu être lu.
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    /// Les métadonnées sont présentes mais illisibles (fichier abîmé ou
    /// variante non prise en charge).
    #[error("unreadable metadata: {0}")]
    Unreadable(String),
}

/// Lit les métadonnées d'une photo ou d'une vidéo.
///
/// Un fichier **sans** métadonnées n'est pas une erreur : le résultat est
/// simplement vide.
///
/// # Errors
///
/// Échoue si le fichier ne peut pas être lu, ou si ses métadonnées sont
/// présentes mais illisibles.
pub fn read_metadata(path: &Path, format: MediaFormat) -> Result<ExtractedMetadata, MetadataError> {
    match format.kind() {
        MediaKind::Photo => photo::read(path, format),
        MediaKind::Video => video::read(path),
    }
}
