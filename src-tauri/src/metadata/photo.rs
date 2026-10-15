//! Métadonnées EXIF des photos.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use nom_exif::{Exif, ExifTag, IfdIndex, IfdKind, MediaParser, MediaSource};

use super::embedded::{JpegExif, jpeg_exif};
use super::recovered::field_value;
use super::values::{capture_time, decimal, gps_position, text, unsigned};
use super::{ExtractedMetadata, MetadataError, RecoveredMetadata};
use crate::domain::{MediaDetails, MediaFormat, MetadataField, Orientation};
use crate::platform;

/// Balise EXIF de l'auteur (`Artist`), absente de la liste de `nom-exif`.
const ARTIST_TAG: u16 = 0x013B;

/// Quantités lues au début du fichier quand la lecture économe échoue, de la
/// plus petite à la plus grande. 256 Kio suffisent à tous nos RAW de test ;
/// les paliers suivants couvrent des fichiers plus inhabituels.
const PREFIX_SIZES: [u64; 4] = [256 * 1024, 1024 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024];

/// Lit l'EXIF d'une photo. Sans nom d'objectif dans l'EXIF, il est demandé
/// au décodeur du système, qui sait le déduire des données propres au
/// fabricant (numéro d'objectif d'un Nikon D7500, par exemple).
pub fn read(path: &Path, format: MediaFormat) -> Result<ExtractedMetadata, MetadataError> {
    let Some(exif) = read_exif(path, format)? else {
        return Ok(ExtractedMetadata::default());
    };
    let mut metadata = extract(&exif, format);
    if metadata.details.lens.is_none() {
        metadata.details.lens = platform::lens_model(path);
    }
    Ok(metadata)
}

/// Lit l'EXIF en deux temps. D'abord une lecture « économe », qui ne lit que
/// les octets nécessaires. Elle échoue sur certains RAW construits comme des
/// TIFF (Canon CR2, Nikon, Sony) : on relit alors le début du fichier en
/// mémoire, par paliers croissants. Elle échoue aussi sur un JPEG dont un
/// segment XMP précède l'EXIF (ou le remplace) : on isole alors nous-mêmes
/// le segment EXIF. Renvoie `None` si le fichier n'a pas d'EXIF.
fn read_exif(path: &Path, format: MediaFormat) -> Result<Option<Exif>, MetadataError> {
    let first_error = match parse(MediaSource::seekable(File::open(path)?)) {
        Ok(exif) => return Ok(Some(exif)),
        Err(error) => match classify(error) {
            Failure::NoExif => return Ok(None),
            Failure::Io(error) => return Err(MetadataError::Io(error)),
            Failure::Retry(message) => message,
        },
    };
    if format == MediaFormat::Jpeg {
        return read_isolated_jpeg_exif(path, first_error);
    }

    let file_size = std::fs::metadata(path)?.len();
    for size in PREFIX_SIZES {
        let mut prefix = Vec::new();
        File::open(path)?.take(size).read_to_end(&mut prefix)?;
        match parse(MediaSource::from_memory(prefix)) {
            Ok(exif) => return Ok(Some(exif)),
            Err(error) => match classify(error) {
                Failure::NoExif => return Ok(None),
                Failure::Io(error) => return Err(MetadataError::Io(error)),
                Failure::Retry(_) if size < file_size => {}
                Failure::Retry(message) => return Err(MetadataError::Unreadable(message)),
            },
        }
    }
    Err(MetadataError::Unreadable(first_error))
}

/// Relit l'EXIF d'un JPEG à partir de son seul segment EXIF, placé dans
/// un JPEG minimal en mémoire (le fichier n'est pas modifié).
fn read_isolated_jpeg_exif(
    path: &Path,
    first_error: String,
) -> Result<Option<Exif>, MetadataError> {
    let segment = match jpeg_exif(path)? {
        JpegExif::Found(segment) => segment,
        JpegExif::Absent => return Ok(None),
        JpegExif::Unknown => return Err(MetadataError::Unreadable(first_error)),
    };
    let Ok(length) = u16::try_from(segment.len() + 2) else {
        return Err(MetadataError::Unreadable(first_error));
    };
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpeg.extend(length.to_be_bytes());
    jpeg.extend(segment);
    jpeg.extend([0xFF, 0xD9]);
    match parse(MediaSource::from_memory(jpeg)) {
        Ok(exif) => Ok(Some(exif)),
        Err(error) => match classify(error) {
            Failure::NoExif => Ok(None),
            Failure::Io(error) => Err(MetadataError::Io(error)),
            Failure::Retry(message) => Err(MetadataError::Unreadable(message)),
        },
    }
}

fn parse<R: Read>(source: nom_exif::Result<MediaSource<R>>) -> nom_exif::Result<Exif> {
    MediaParser::new().parse_exif(source?).map(Exif::from)
}

/// Ce qu'on fait d'une erreur de `nom-exif`.
enum Failure {
    /// Le fichier ne contient pas d'EXIF : ce n'est pas une erreur.
    NoExif,
    /// Le fichier ne peut pas être lu.
    Io(std::io::Error),
    /// EXIF mal lu : on retente avec plus de données (message conservé).
    Retry(String),
}

fn classify(error: nom_exif::Error) -> Failure {
    match error {
        nom_exif::Error::ExifNotFound | nom_exif::Error::UnsupportedFormat => Failure::NoExif,
        nom_exif::Error::Io(error) => Failure::Io(error),
        other => Failure::Retry(other.to_string()),
    }
}

fn extract(exif: &Exif, format: MediaFormat) -> ExtractedMetadata {
    let capture_time = capture_time(
        exif.get(ExifTag::DateTimeOriginal),
        exif.get(ExifTag::OffsetTimeOriginal),
    )
    .or_else(|| {
        capture_time(
            exif.get(ExifTag::CreateDate),
            exif.get(ExifTag::OffsetTimeDigitized),
        )
    });
    let (width, height) = dimensions(exif, format);
    let gps = exif.gps_info().and_then(gps_position);

    ExtractedMetadata {
        capture_time,
        recovered: RecoveredMetadata {
            fields: descriptive_fields(exif),
            gps,
            ..RecoveredMetadata::default()
        },
        details: MediaDetails {
            width,
            height,
            orientation: unsigned(exif.get(ExifTag::Orientation))
                .and_then(|value| u8::try_from(value).ok())
                .and_then(Orientation::new),
            camera_make: text(exif.get(ExifTag::Make)),
            camera_model: text(exif.get(ExifTag::Model)),
            lens: text(exif.get(ExifTag::LensModel)),
            focal_length_mm: decimal(exif.get(ExifTag::FocalLength)),
            aperture: decimal(exif.get(ExifTag::FNumber)),
            exposure_time_s: decimal(exif.get(ExifTag::ExposureTime)),
            iso: unsigned(exif.get(ExifTag::ISOSpeedRatings))
                .and_then(|value| u32::try_from(value).ok()),
            // Bit 0 du champ Flash : le flash s'est déclenché.
            flash_fired: unsigned(exif.get(ExifTag::Flash)).map(|value| value & 1 == 1),
            gps,
            duration_ms: None,
        },
    }
}

/// Auteur et copyright, souvent réglés dans le boîtier. La description
/// (`ImageDescription`) est écartée : beaucoup d'appareils y écrivent leur
/// nom (« OLYMPUS DIGITAL CAMERA »).
fn descriptive_fields(exif: &Exif) -> Vec<(MetadataField, String)> {
    [
        (MetadataField::Creator, ARTIST_TAG),
        (MetadataField::Copyright, ExifTag::Copyright.code()),
    ]
    .into_iter()
    .filter_map(|(field, code)| {
        let raw = text(exif.get_by_code_in(IfdIndex::MAIN, IfdKind::Tiff, code))?;
        // Le copyright EXIF sépare photographe et éditeur par un octet nul.
        let parts: Vec<&str> = raw
            .split('\0')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        Some((field, field_value(field, &parts.join(", "))?))
    })
    .collect()
}

/// Dimensions de l'image. Dans un RAW, les champs `ImageWidth`/`ImageHeight`
/// décrivent souvent la petite vignette intégrée, pas la photo : on ne s'y
/// fie que pour les autres formats. Des dimensions absentes valent mieux que
/// des dimensions fausses ; elles seront complétées au décodage de l'image.
fn dimensions(exif: &Exif, format: MediaFormat) -> (Option<u32>, Option<u32>) {
    let pixel = |tag| unsigned(exif.get(tag)).and_then(|value| u32::try_from(value).ok());
    let exif_dimensions = (
        pixel(ExifTag::ExifImageWidth),
        pixel(ExifTag::ExifImageHeight),
    );
    match exif_dimensions {
        (Some(width), Some(height)) => (Some(width), Some(height)),
        _ if format.is_raw() => (None, None),
        _ => (pixel(ExifTag::ImageWidth), pixel(ExifTag::ImageHeight)),
    }
}
