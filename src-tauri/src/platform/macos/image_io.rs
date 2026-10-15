//! Vignettes avec ImageIO, le moteur d'images de macOS (celui d'Aperçu et du
//! Finder) : il lit tous nos formats photo, HEIC et RAW compris, et sait
//! utiliser l'aperçu JPEG intégré aux fichiers RAW.
//!
//! Les fonctions d'ImageIO sont appelées à travers les liaisons `objc2`, qui
//! les déclarent `unsafe` parce que leurs dictionnaires d'options ne sont pas
//! typés. Le code `unsafe` n'est permis que dans les fichiers qui appellent
//! les moteurs de macOS (celui-ci et `av_foundation.rs`) ; chaque appel est
//! justifié par un commentaire `SAFETY:`.
#![allow(unsafe_code)]

use std::ops::Deref;
use std::path::Path;

use objc2::rc::Retained;

use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType, CFURL,
};
use objc2_core_graphics::CGImage;
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
    kCGImagePropertyExifAuxDictionary, kCGImagePropertyExifAuxLensModel,
    kCGImagePropertyExifDictionary, kCGImagePropertyExifLensModel, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailFromImageIfAbsent, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceShouldCache, kCGImageSourceThumbnailMaxPixelSize,
};

use super::core_image;
use crate::platform::{
    FullSizeImage, FullSizeRequest, ImageEngine, ThumbnailError, ThumbnailInfo, ThumbnailRequest,
};

/// Lit une constante d'ImageIO (clé d'option ou de propriété).
macro_rules! image_io_key {
    ($name:ident) => {{
        // SAFETY: les clés d'ImageIO sont des chaînes immuables fournies par
        // le système et initialisées avant tout appel ; Rust exige seulement
        // un bloc `unsafe` pour lire une variable externe.
        unsafe { $name }
    }};
}

/// Type uniforme (UTI) du format JPEG, pour l'écriture.
const JPEG_TYPE: &str = "public.jpeg";

/// Taille demandée quand celle de la photo est inconnue : plus grande que
/// n'importe quel capteur, pour obtenir l'image à sa taille réelle.
const FULL_SIZE_FALLBACK: u32 = 100_000;

/// Fabrique la vignette décrite par `request`.
pub fn make_thumbnail(request: &ThumbnailRequest<'_>) -> Result<ThumbnailInfo, ThumbnailError> {
    let source = open_source(request.source)?;
    let (source_width, source_height) = pixel_dimensions(&source);

    // Aperçu intégré (RAW) : rapide. Il peut être trop petit (vignette EXIF
    // de 160 pixels) : on décode alors l'image elle-même.
    let preview = embedded_preview(
        &source,
        request.prefer_embedded_preview,
        request.max_pixel_size,
        request.max_pixel_size,
        (source_width, source_height),
    );
    let used_embedded_preview = preview.is_some();
    let image = match preview {
        Some(preview) => Decoded::ImageIo(preview),
        None => decode(
            request.source,
            &source,
            Some(request.max_pixel_size),
            request.engine,
            request.prefer_embedded_preview,
        )?,
    };

    write_jpeg(&image, request.destination, request.jpeg_quality)?;
    Ok(ThumbnailInfo {
        width: dimension(CGImage::width(Some(&image))),
        height: dimension(CGImage::height(Some(&image))),
        source_width,
        source_height,
        used_embedded_preview,
    })
}

/// Fabrique la photo entière en JPEG, en mémoire (voir [`FullSizeRequest`]).
pub fn make_full_size_jpeg(request: &FullSizeRequest<'_>) -> Result<FullSizeImage, ThumbnailError> {
    let source = open_source(request.source)?;
    let (source_width, source_height) = pixel_dimensions(&source);
    // Sans dimensions connues, une taille démesurée laisse ImageIO produire
    // l'image à sa taille réelle (jamais agrandie).
    let full_size = source_width
        .max(source_height)
        .unwrap_or(FULL_SIZE_FALLBACK);

    let preview = embedded_preview(
        &source,
        request.prefer_embedded_preview,
        full_size,
        request.min_embedded_preview_size,
        (source_width, source_height),
    );
    let image = match preview {
        Some(preview) => Decoded::ImageIo(preview),
        None => decode(
            request.source,
            &source,
            None,
            request.engine,
            request.prefer_embedded_preview,
        )?,
    };
    Ok(FullSizeImage {
        jpeg: encode_jpeg(&image, request.jpeg_quality)?,
        width: dimension(CGImage::width(Some(&image))),
        height: dimension(CGImage::height(Some(&image))),
    })
}

/// Image décodée, par ImageIO ou par Core Image (types de retour différents
/// pour la même image Core Graphics).
enum Decoded {
    ImageIo(CFRetained<CGImage>),
    CoreImage(Retained<CGImage>),
}

impl Deref for Decoded {
    type Target = CGImage;

    fn deref(&self) -> &CGImage {
        match self {
            Self::ImageIo(image) => image,
            Self::CoreImage(image) => image,
        }
    }
}

/// Aperçu intégré au fichier, s'il est demandé et assez grand (au moins
/// `min_size`, ou la taille de la photo si elle est plus petite).
fn embedded_preview(
    source: &CGImageSource,
    wanted: bool,
    max_pixel_size: u32,
    min_size: u32,
    (source_width, source_height): (Option<u32>, Option<u32>),
) -> Option<CFRetained<CGImage>> {
    if !wanted {
        return None;
    }
    thumbnail(source, max_pixel_size, false)
        .filter(|preview| large_enough(preview, min_size, source_width, source_height))
}

/// Décode la photo elle-même, réduite à `max_pixel_size` (`None` : taille
/// réelle) : sur le GPU avec Core Image si demandé (décodage complet pour un
/// RAW), sinon ou en cas d'échec sur le processeur avec ImageIO.
fn decode(
    path: &Path,
    source: &CGImageSource,
    max_pixel_size: Option<u32>,
    engine: ImageEngine,
    is_raw: bool,
) -> Result<Decoded, ThumbnailError> {
    let size = max_pixel_size.unwrap_or(FULL_SIZE_FALLBACK);
    if engine == ImageEngine::Gpu && core_image::is_available() {
        let decoded = if is_raw {
            core_image::decode_raw(path, max_pixel_size)
        } else {
            core_image::downscale(path, size)
        };
        match decoded {
            Ok(image) => return Ok(Decoded::CoreImage(image)),
            // ImageIO saura peut-être : on retente sur le processeur.
            Err(error) => {
                tracing::debug!(%error, path = %path.display(), "GPU decoding failed, using the CPU");
            }
        }
    }
    thumbnail(source, size, true)
        .map(Decoded::ImageIo)
        .ok_or(ThumbnailError::Undecodable)
}

fn open_source(path: &Path) -> Result<CFRetained<CGImageSource>, ThumbnailError> {
    let url = CFURL::from_file_path(path).ok_or(ThumbnailError::InvalidPath)?;
    // On demande à ImageIO de ne pas garder l'image décodée en mémoire : chaque
    // fichier n'est lu qu'une fois.
    let options = dictionary(&[(
        image_io_key!(kCGImageSourceShouldCache),
        CFBoolean::new(false).as_ref(),
    )]);
    // SAFETY: `options` n'associe que des clés ImageIO documentées à des
    // valeurs du type attendu (ici un booléen).
    unsafe { CGImageSource::with_url(&url, Some(options.as_opaque())) }
        .ok_or(ThumbnailError::Undecodable)
}

/// Vignette dont le plus grand côté mesure au plus `max_pixel_size`, orientée
/// selon l'EXIF. `decode_image` : décoder l'image elle-même plutôt que
/// d'utiliser l'aperçu intégré au fichier.
fn thumbnail(
    source: &CGImageSource,
    max_pixel_size: u32,
    decode_image: bool,
) -> Option<CFRetained<CGImage>> {
    let creation_key = if decode_image {
        image_io_key!(kCGImageSourceCreateThumbnailFromImageAlways)
    } else {
        image_io_key!(kCGImageSourceCreateThumbnailFromImageIfAbsent)
    };
    let size = CFNumber::new_i64(i64::from(max_pixel_size));
    let options = dictionary(&[
        (creation_key, CFBoolean::new(true).as_ref()),
        (
            image_io_key!(kCGImageSourceCreateThumbnailWithTransform),
            CFBoolean::new(true).as_ref(),
        ),
        (
            image_io_key!(kCGImageSourceThumbnailMaxPixelSize),
            size.as_ref(),
        ),
    ]);
    // SAFETY: l'index 0 est la photo principale ; `options` n'associe que des
    // clés ImageIO documentées à des valeurs du type attendu (booléens et
    // nombre entier).
    unsafe { source.thumbnail_at_index(0, Some(options.as_opaque())) }
}

/// L'aperçu est-il assez grand ? Oui s'il atteint la taille demandée, ou si
/// la photo elle-même est plus petite que cette taille.
fn large_enough(
    image: &CGImage,
    max_pixel_size: u32,
    source_width: Option<u32>,
    source_height: Option<u32>,
) -> bool {
    let longest = CGImage::width(Some(image)).max(CGImage::height(Some(image)));
    let wanted = usize::try_from(max_pixel_size).unwrap_or(usize::MAX);
    let source_longest = source_width
        .max(source_height)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX);
    longest >= wanted.min(source_longest)
}

/// Dimensions de la photo en pixels, lues dans ses propriétés.
fn pixel_dimensions(source: &CGImageSource) -> (Option<u32>, Option<u32>) {
    // SAFETY: l'index 0 est la photo principale ; aucune option n'est passée.
    let Some(properties) = (unsafe { source.properties_at_index(0, None) }) else {
        return (None, None);
    };
    // SAFETY: les propriétés d'ImageIO sont un dictionnaire dont les clés sont
    // des chaînes CoreFoundation ; les valeurs sont vérifiées une à une
    // (`downcast_ref`) avant utilisation.
    let properties = unsafe { properties.cast_unchecked::<CFString, CFType>() };
    let read = |key: &CFString| {
        properties
            .get(key)
            .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64))
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
    };
    (
        read(image_io_key!(kCGImagePropertyPixelWidth)),
        read(image_io_key!(kCGImagePropertyPixelHeight)),
    )
}

/// Nom de l'objectif tel qu'ImageIO le lit : le champ EXIF `LensModel`, ou
/// à défaut celui qu'il déduit des données propres au fabricant (un Nikon
/// D7500 n'y note qu'un numéro d'objectif). `None` si rien n'est connu.
#[must_use]
pub fn lens_model(path: &Path) -> Option<String> {
    let source = open_source(path).ok()?;
    // SAFETY: l'index 0 est la photo principale ; aucune option n'est passée.
    let properties = unsafe { source.properties_at_index(0, None) }?;
    // SAFETY: comme pour `pixel_dimensions`, les clés sont des chaînes
    // CoreFoundation et chaque valeur est vérifiée avant utilisation.
    let properties = unsafe { properties.cast_unchecked::<CFString, CFType>() };
    let lens_in = |dictionary: &CFString, key: &CFString| {
        let group = properties.get(dictionary)?;
        let group = group.downcast_ref::<CFDictionary>()?;
        // SAFETY: un groupe de propriétés d'ImageIO est lui aussi un
        // dictionnaire à clés CoreFoundation ; la valeur est vérifiée.
        let group = unsafe { group.cast_unchecked::<CFString, CFType>() };
        let name = group.get(key)?.downcast_ref::<CFString>()?.to_string();
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_owned())
    };
    lens_in(
        image_io_key!(kCGImagePropertyExifDictionary),
        image_io_key!(kCGImagePropertyExifLensModel),
    )
    .or_else(|| {
        lens_in(
            image_io_key!(kCGImagePropertyExifAuxDictionary),
            image_io_key!(kCGImagePropertyExifAuxLensModel),
        )
    })
}

/// Écrit l'image en JPEG. Le fichier de destination ne doit pas exister
/// (le module `thumbnails` écrit dans un fichier temporaire, puis le renomme).
pub(super) fn write_jpeg(
    image: &CGImage,
    destination: &Path,
    quality: f64,
) -> Result<(), ThumbnailError> {
    let url = CFURL::from_file_path(destination).ok_or(ThumbnailError::InvalidPath)?;
    let jpeg = CFString::from_static_str(JPEG_TYPE);
    // SAFETY: une seule image sera ajoutée (`count` = 1) ; aucune option.
    let writer = unsafe { CGImageDestination::with_url(&url, &jpeg, 1, None) }
        .ok_or(ThumbnailError::WriteFailed)?;
    add_and_finalize(&writer, image, quality)
}

/// Encode l'image en JPEG, en mémoire.
fn encode_jpeg(image: &CGImage, quality: f64) -> Result<Vec<u8>, ThumbnailError> {
    let data = CFMutableData::new(None, 0).ok_or(ThumbnailError::WriteFailed)?;
    let jpeg = CFString::from_static_str(JPEG_TYPE);
    // SAFETY: une seule image sera ajoutée (`count` = 1) ; aucune option.
    let writer = unsafe { CGImageDestination::with_data(&data, &jpeg, 1, None) }
        .ok_or(ThumbnailError::WriteFailed)?;
    add_and_finalize(&writer, image, quality)?;
    Ok(data.to_vec())
}

/// Ajoute l'unique image à écrire, avec sa qualité JPEG, et termine
/// l'écriture.
fn add_and_finalize(
    writer: &CGImageDestination,
    image: &CGImage,
    quality: f64,
) -> Result<(), ThumbnailError> {
    let quality = CFNumber::new_f64(quality);
    let properties = dictionary(&[(
        image_io_key!(kCGImageDestinationLossyCompressionQuality),
        quality.as_ref(),
    )]);
    // SAFETY: `properties` n'associe que la qualité de compression, un nombre
    // entre 0 et 1, comme le demande ImageIO.
    unsafe { writer.add_image(image, Some(properties.as_opaque())) };
    // SAFETY: l'unique image annoncée a été ajoutée juste avant.
    if unsafe { writer.finalize() } {
        Ok(())
    } else {
        Err(ThumbnailError::WriteFailed)
    }
}

/// Dictionnaire d'options ImageIO.
fn dictionary(entries: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
    let (keys, values): (Vec<&CFString>, Vec<&CFType>) = entries.iter().copied().unzip();
    CFDictionary::from_slices(&keys, &values)
}

fn dimension(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
