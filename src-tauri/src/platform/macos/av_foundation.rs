//! Vignettes des vidéos avec AVFoundation, le moteur vidéo de macOS (celui de
//! QuickTime) : MOV, MP4, M4V, 3GP. L'image extraite est celle du milieu de
//! la vidéo, tournée comme à la lecture (vidéos filmées en portrait).
//!
//! Comme pour ImageIO, les liaisons `objc2` déclarent ces appels `unsafe` :
//! ce fichier fait partie des seuls où c'est permis, et chaque bloc est
//! justifié par un commentaire `SAFETY:`.
#![allow(unsafe_code)]

use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
use objc2_core_foundation::{CFRetained, CGSize};
use objc2_core_graphics::CGImage;
use objc2_core_media::CMTime;
use objc2_foundation::{NSError, NSURL};

use super::image_io::write_jpeg;
use crate::platform::{ThumbnailError, ThumbnailInfo, ThumbnailRequest};

/// Délai maximal pour extraire une image : au-delà, la vidéo est considérée
/// comme illisible plutôt que de bloquer la tâche de fond.
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Précision des temps demandés à AVFoundation (600 unités par seconde, la
/// valeur habituelle pour la vidéo).
const TIMESCALE: i32 = 600;

/// Écart toléré autour du milieu : AVFoundation peut prendre l'image clé la
/// plus proche, ce qui est bien plus rapide que de décoder l'image exacte.
const TOLERANCE_SECONDS: f64 = 0.5;

/// Fabrique la vignette d'une vidéo.
pub fn make_thumbnail(request: &ThumbnailRequest<'_>) -> Result<ThumbnailInfo, ThumbnailError> {
    let url = NSURL::from_file_path(request.source).ok_or(ThumbnailError::InvalidPath)?;
    // SAFETY: `url` désigne un fichier ; aucune option n'est passée.
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
    // SAFETY: `asset` est une vidéo valide (éventuellement illisible, ce que
    // signale une durée nulle ou une extraction en échec).
    let generator = unsafe { AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset) };
    let size = f64::from(request.max_pixel_size);
    // SAFETY: réglages simples du générateur, avant toute extraction.
    unsafe {
        generator.setAppliesPreferredTrackTransform(true);
        generator.setMaximumSize(CGSize {
            width: size,
            height: size,
        });
        let tolerance = CMTime::with_seconds(TOLERANCE_SECONDS, TIMESCALE);
        generator.setRequestedTimeToleranceBefore(tolerance);
        generator.setRequestedTimeToleranceAfter(tolerance);
    }

    // SAFETY: lecture de la durée d'une vidéo ouverte ; `seconds` convertit
    // un temps CoreMedia valide ou invalide (le résultat est alors NaN).
    let seconds = unsafe { asset.duration().seconds() };
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(ThumbnailError::Undecodable);
    }
    // SAFETY: création d'un temps CoreMedia à partir d'un nombre fini.
    let middle = unsafe { CMTime::with_seconds(seconds / 2.0, TIMESCALE) };

    let image = extract(&generator, middle)?;
    write_jpeg(&image, request.destination, request.jpeg_quality)?;
    Ok(ThumbnailInfo {
        width: dimension(CGImage::width(Some(&image))),
        height: dimension(CGImage::height(Some(&image))),
        source_width: None,
        source_height: None,
        used_embedded_preview: false,
    })
}

/// Extrait l'image au temps demandé. AVFoundation répond plus tard, sur un
/// autre fil : on attend sa réponse, au plus [`EXTRACTION_TIMEOUT`].
fn extract(
    generator: &AVAssetImageGenerator,
    time: CMTime,
) -> Result<CFRetained<CGImage>, ThumbnailError> {
    let (sender, receiver) = mpsc::channel::<Option<CFRetained<CGImage>>>();
    let handler = RcBlock::new(
        move |image: *mut CGImage, _actual_time: CMTime, _error: *mut NSError| {
            let image = NonNull::new(image).map(|image| {
                // SAFETY: AVFoundation prête une image valide le temps de l'appel ;
                // on la retient pour la garder après.
                unsafe { CFRetained::retain(image) }
            });
            // Si l'envoi échoue, c'est que l'attente a déjà expiré : l'image n'est
            // plus attendue.
            let _unused = sender.send(image);
        },
    );
    // SAFETY: `handler` reste valide jusqu'à son appel (AVFoundation le
    // retient) et accepte les trois paramètres documentés.
    unsafe { generator.generateCGImageAsynchronouslyForTime_completionHandler(time, &handler) };

    match receiver.recv_timeout(EXTRACTION_TIMEOUT) {
        Ok(Some(image)) => Ok(image),
        Ok(None) => Err(ThumbnailError::Undecodable),
        Err(_) => {
            // SAFETY: annulation des extractions en cours de ce générateur.
            unsafe { generator.cancelAllCGImageGeneration() };
            Err(ThumbnailError::Undecodable)
        }
    }
}

fn dimension(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
