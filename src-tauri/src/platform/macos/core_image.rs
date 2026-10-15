//! Traitement d'images sur le GPU avec Core Image, le moteur d'images de
//! macOS qui s'appuie sur Metal : décodage complet des RAW et réduction des
//! grandes images.
//!
//! Un seul contexte Core Image est créé, au premier besoin, sur le GPU par
//! défaut du Mac ; il est partagé par tous les fils de fabrication. Sans GPU
//! utilisable (machine virtuelle...), aucune fonction de ce module n'est
//! appelée : les images restent traitées par ImageIO, sur le processeur.
//!
//! Comme pour ImageIO et AVFoundation, les liaisons `objc2` déclarent ces
//! appels `unsafe` : ce fichier fait partie des seuls où c'est permis, et
//! chaque bloc est justifié par un commentaire `SAFETY:`.
#![allow(unsafe_code)]

use std::path::Path;
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::CGRect;
use objc2_core_graphics::{CGColorSpace, CGColorSpaceModel, CGImage, kCGColorSpaceSRGB};
use objc2_core_image::{
    CIContext, CIImage, CIRAWFilter, kCIContextCacheIntermediates, kCIFormatRGBA8,
    kCIImageApplyOrientationProperty,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

use crate::platform::ThumbnailError;

/// Lit une clé d'option de Core Image.
macro_rules! core_image_key {
    ($name:ident) => {{
        // SAFETY: les clés de Core Image sont des chaînes immuables fournies
        // par le système et initialisées avant tout appel ; Rust exige
        // seulement un bloc `unsafe` pour lire une variable externe.
        unsafe { $name }
    }};
}

/// Filtre de réduction de haute qualité de Core Image.
const LANCZOS_FILTER: &str = "CILanczosScaleTransform";

/// Contexte Core Image partagé.
struct GpuContext(Retained<CIContext>);

// SAFETY: Apple documente les objets `CIContext` comme immuables et sûrs à
// partager entre fils d'exécution (« CIContext and CIImage objects are
// immutable, which means each can be shared safely among threads »).
unsafe impl Send for GpuContext {}
// SAFETY: voir ci-dessus.
unsafe impl Sync for GpuContext {}

static CONTEXT: OnceLock<Option<GpuContext>> = OnceLock::new();

/// Contexte Core Image sur le GPU, créé au premier appel ; `None` si le Mac
/// n'a pas de GPU utilisable.
fn context() -> Option<&'static CIContext> {
    CONTEXT
        .get_or_init(|| {
            let Some(device) = MTLCreateSystemDefaultDevice() else {
                tracing::info!("no GPU available: images are processed by the CPU");
                return None;
            };
            tracing::info!(gpu = %device.name(), "GPU used for image processing");
            // Chaque image n'est traitée qu'une fois : garder les résultats
            // intermédiaires ne ferait qu'occuper la mémoire du GPU.
            let no = NSNumber::new_bool(false);
            let options = NSDictionary::from_slices(
                &[core_image_key!(kCIContextCacheIntermediates)],
                &[no.as_ref() as &AnyObject],
            );
            // SAFETY: `options` n'associe qu'une clé Core Image documentée à
            // une valeur du type attendu (un booléen).
            let context =
                unsafe { CIContext::contextWithMTLDevice_options(&device, Some(&options)) };
            Some(GpuContext(context))
        })
        .as_ref()
        .map(|context| &*context.0)
}

/// Indique si le GPU est utilisable pour traiter les images.
pub fn is_available() -> bool {
    context().is_some()
}

/// Décode entièrement un RAW sur le GPU (rendu d'Apple, orientation
/// appliquée), réduit pour que son plus grand côté mesure au plus
/// `max_pixel_size` ; `None` : taille réelle du capteur.
pub fn decode_raw(
    source: &Path,
    max_pixel_size: Option<u32>,
) -> Result<Retained<CGImage>, ThumbnailError> {
    let context = context().ok_or(ThumbnailError::Unsupported)?;
    let url = NSURL::from_file_path(source).ok_or(ThumbnailError::InvalidPath)?;
    // SAFETY: `url` désigne un fichier ; Core Image renvoie `None` s'il ne
    // sait pas le lire comme un RAW.
    let filter =
        unsafe { CIRAWFilter::filterWithImageURL(&url) }.ok_or(ThumbnailError::Undecodable)?;
    if let Some(max) = max_pixel_size {
        // SAFETY: lecture de la taille d'un RAW ouvert.
        let native = unsafe { filter.nativeSize() };
        let scale = scale_to_fit(native.width.max(native.height), max);
        if scale < 1.0 {
            // Le décodeur travaille directement à cette échelle, bien plus
            // vite qu'en décodant tout puis en réduisant.
            // SAFETY: réglage d'un filtre RAW, avant de demander l'image.
            #[allow(clippy::cast_possible_truncation)]
            unsafe {
                filter.setScaleFactor(scale as f32);
            }
        }
    }
    // SAFETY: l'image de sortie du filtre RAW, décrite mais pas encore
    // calculée ; `None` si le décodage est impossible.
    let image = unsafe { filter.outputImage() }.ok_or(ThumbnailError::Undecodable)?;
    render(context, &image)
}

/// Lit une image (JPEG, HEIC...) et la réduit sur le GPU pour que son plus
/// grand côté mesure au plus `max_pixel_size`, orientation EXIF appliquée.
pub fn downscale(source: &Path, max_pixel_size: u32) -> Result<Retained<CGImage>, ThumbnailError> {
    let context = context().ok_or(ThumbnailError::Unsupported)?;
    let url = NSURL::from_file_path(source).ok_or(ThumbnailError::InvalidPath)?;
    let yes = NSNumber::new_bool(true);
    let options = NSDictionary::from_slices(
        &[core_image_key!(kCIImageApplyOrientationProperty)],
        &[yes.as_ref() as &AnyObject],
    );
    // SAFETY: `options` n'associe qu'une clé Core Image documentée à une
    // valeur du type attendu (un booléen).
    let image = unsafe { CIImage::imageWithContentsOfURL_options(&url, Some(&options)) }
        .ok_or(ThumbnailError::Undecodable)?;
    // SAFETY: lecture de l'étendue d'une image.
    let extent = unsafe { image.extent() };
    let scale = scale_to_fit(extent.size.width.max(extent.size.height), max_pixel_size);
    let image = if scale < 1.0 {
        let scale_value = NSNumber::new_f64(scale);
        let aspect_ratio = NSNumber::new_f64(1.0);
        let parameters = NSDictionary::from_slices(
            &[
                &*NSString::from_str("inputScale"),
                &*NSString::from_str("inputAspectRatio"),
            ],
            &[
                scale_value.as_ref() as &AnyObject,
                aspect_ratio.as_ref() as &AnyObject,
            ],
        );
        // SAFETY: le filtre de réduction standard de Core Image, avec ses
        // deux paramètres documentés (nombres).
        unsafe {
            image.imageByApplyingFilter_withInputParameters(
                &NSString::from_str(LANCZOS_FILTER),
                &parameters,
            )
        }
    } else {
        image
    };
    render(context, &image)
}

/// Calcule l'image sur le GPU et la rapatrie sous forme de `CGImage`, prête
/// à être encodée en JPEG. L'espace colorimétrique de la photo est conservé
/// (Display P3 des iPhone, par exemple) ; à défaut (RAW), sRGB.
fn render(context: &CIContext, image: &CIImage) -> Result<Retained<CGImage>, ThumbnailError> {
    // SAFETY: lecture de l'étendue d'une image.
    let extent: CGRect = unsafe { image.extent() };
    // SAFETY: lecture de l'espace colorimétrique d'une image.
    let own = unsafe { image.colorSpace() }
        .filter(|space| CGColorSpace::model(Some(space)) == CGColorSpaceModel::RGB);
    // SAFETY: lecture d'une constante du système (voir `core_image_key!`).
    let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
    let color_space: Option<&CGColorSpace> = own.as_deref().or(srgb.as_deref());
    // SAFETY: l'étendue est celle de l'image elle-même ; le format RGBA 8 bits
    // et un espace RGB sont compatibles, comme l'exige Core Image, qui renvoie
    // `None` si l'image est vide ou trop grande.
    unsafe {
        context.createCGImage_fromRect_format_colorSpace(image, extent, kCIFormatRGBA8, color_space)
    }
    .ok_or(ThumbnailError::Undecodable)
}

/// Facteur de réduction pour que `longest` tienne dans `max_pixel_size`
/// (1 si l'image est déjà assez petite).
fn scale_to_fit(longest: f64, max_pixel_size: u32) -> f64 {
    if longest <= 0.0 {
        return 1.0;
    }
    (f64::from(max_pixel_size) / longest).min(1.0)
}
