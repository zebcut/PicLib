//! Vignettes des photos et des vidéos : fabrication en tâche de fond et cache
//! sur disque.
//!
//! Les vignettes (512 pixels sur le plus grand côté, JPEG) sont rangées dans
//! le dossier `Thumbnails` du dossier de travail. Elles sont fabriquées par le
//! décodeur du système (ImageIO sur macOS, voir [`crate::platform`]), à
//! partir de l'aperçu intégré pour les RAW ; celles des vidéos par son moteur
//! vidéo (AVFoundation), à partir de l'image du milieu.
//!
//! Les aperçus de la Loupe (2560 pixels) sont fabriqués de la même façon, à
//! la demande, dans un cache limité en taille (`Aperçus`, voir).
//!
//! Les vignettes de l'écran d'import, fabriquées à partir des fichiers de la
//! source avant tout import, ont leur propre cache temporaire.

mod cache;
mod generator;
mod import_preview;
mod preview;

pub use cache::{CacheError, JPEG_QUALITY, THUMBNAIL_SIZE, ThumbnailCache};
pub use generator::{GenerationError, GenerationReport, generate_pending};
pub use import_preview::{IMPORT_THUMBNAIL_SIZE, ImportPreviewCache};
pub use preview::{PREVIEW_JPEG_QUALITY, PREVIEW_SIZE, PreviewCache};
