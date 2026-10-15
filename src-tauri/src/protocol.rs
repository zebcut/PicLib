//! Protocole `piclib://` : l'interface y demande les images à afficher
//! (`piclib://localhost/thumbnail/123`, ou
//! `http://piclib.localhost/thumbnail/123` sous Windows) :
//!
//! - `thumbnail/<id>` : la vignette (512 pixels), déjà fabriquée ;
//! - `preview/<id>` : l'aperçu de la Loupe (2560 pixels), fabriqué au besoin ;
//! - `original/<id>` : la photo en pleine résolution, pour le zoom à 100 % ;
//! - `import/<analyse>/<place>` : la vignette d'un fichier d'une source,
//!   dans l'écran d'import, fabriquée au besoin ;
//! - `tile/<z>/<x>/<y>` : une tuile du fond de carte,
//!   téléchargée au besoin et gardée en cache.
//!
//! Le cœur vérifie la demande : seul un numéro de média est accepté, et
//! seuls les fichiers de ses caches, ceux inscrits au catalogue et ceux d'une
//! source analysée sont lus,
//! jamais modifiés. L'interface n'a aucun accès direct aux fichiers.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use tauri::http::{HeaderValue, Request, Response, StatusCode, Uri, header};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder};

use crate::catalog::Catalog;
use crate::catalog::repositories::library::{self, MediaFile};
use crate::commands::folder_analysis::AnalyzedFolders;
use crate::domain::{MediaFormat, MediaId, MediaKind};
use crate::map_tiles::{MapTileCache, TileCoord};
use crate::platform::{self, FullSizeRequest, ImagePurpose, ThumbnailError};
use crate::thumbnails::{
    CacheError, ImportPreviewCache, PREVIEW_SIZE, PreviewCache, ThumbnailCache,
};
use crate::workspace::Workspace;

/// Nom du protocole.
pub const SCHEME: &str = "piclib";

/// Qualité JPEG d'une photo convertie pour le zoom à 100 % (RAW) : proche de
/// l'original, l'image n'étant pas gardée sur le disque.
const FULL_SIZE_JPEG_QUALITY: f64 = 0.92;

/// Image demandée par l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    /// Vignette d'un média.
    Thumbnail(MediaId),
    /// Aperçu d'un média pour la Loupe.
    Preview(MediaId),
    /// Photo en pleine résolution (zoom à 100 %).
    Original(MediaId),
    /// Tuile du fond de carte (vue Carte).
    MapTile(TileCoord),
    /// Vignette d'un fichier d'une source analysée (écran d'import).
    ImportThumbnail {
        /// Tâche d'analyse.
        analysis: u32,
        /// Place du fichier dans l'analyse.
        index: usize,
    },
}

/// Répond à une demande adressée au protocole, sans bloquer le fil
/// principal : la lecture (et au besoin la fabrication) de l'image se fait
/// sur un fil d'arrière-plan.
// La signature est imposée par Tauri, qui transmet le contexte par valeur.
#[allow(clippy::needless_pass_by_value)]
pub fn handle<R: tauri::Runtime>(
    context: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        responder.respond(respond(&app, request.uri()));
    });
}

fn respond<R: tauri::Runtime>(app: &AppHandle<R>, uri: &Uri) -> Response<Vec<u8>> {
    let media_id = match requested_resource(uri) {
        None => return status(StatusCode::NOT_FOUND),
        Some(Resource::Thumbnail(media_id)) => {
            let cache = app
                .try_state::<Workspace>()
                .map(|workspace| ThumbnailCache::new(workspace.thumbnails_dir()));
            return thumbnail_response(cache.as_ref(), media_id);
        }
        Some(Resource::ImportThumbnail { analysis, index }) => {
            return import_thumbnail_response(app, analysis, index);
        }
        Some(Resource::MapTile(tile)) => {
            let Some(tiles) = app.try_state::<MapTileCache>() else {
                return status(StatusCode::NOT_FOUND);
            };
            return match tiles.get(tile) {
                Ok(bytes) => image(bytes, "image/png"),
                Err(error) => {
                    tracing::debug!(%error, "map tile unavailable");
                    status(StatusCode::SERVICE_UNAVAILABLE)
                }
            };
        }
        Some(Resource::Preview(media_id) | Resource::Original(media_id)) => media_id,
    };
    let (Some(file), Some(previews)) = (media_file(app, media_id), app.try_state::<PreviewCache>())
    else {
        return status(StatusCode::NOT_FOUND);
    };
    if matches!(requested_resource(uri), Some(Resource::Original(_))) {
        original_response(&previews, media_id, &file)
    } else {
        preview_response(&previews, media_id, &file)
    }
}

/// Fichier d'un média, lu au catalogue. Le verrou du catalogue n'est gardé
/// que le temps de cette lecture, pas pendant la fabrication de l'image.
fn media_file<R: tauri::Runtime>(app: &AppHandle<R>, media_id: MediaId) -> Option<MediaFile> {
    let catalog = app.try_state::<Mutex<Catalog>>()?;
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    library::media_file(catalog.connection(), media_id)
        .inspect_err(|error| tracing::error!(%error, "cannot read a media file from the catalog"))
        .ok()
        .flatten()
}

/// Réponse à une demande de vignette de l'écran d'import : fabriquée au
/// premier affichage à partir du fichier de la source, seulement lu.
fn import_thumbnail_response<R: tauri::Runtime>(
    app: &AppHandle<R>,
    analysis: u32,
    index: usize,
) -> Response<Vec<u8>> {
    let file = app
        .try_state::<AnalyzedFolders>()
        .and_then(|analyzed| analyzed.file(analysis, index));
    let (Some((source, format)), Some(previews)) = (file, app.try_state::<ImportPreviewCache>())
    else {
        return status(StatusCode::NOT_FOUND);
    };
    let read = previews
        .get_or_create(analysis, index, &source, format)
        .map_err(|error| failure_status(&error, &source))
        .and_then(|path| std::fs::read(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR));
    match read {
        Ok(bytes) => image(bytes, "image/jpeg"),
        Err(code) => status(code),
    }
}

/// Réponse à une demande de vignette : la vignette si elle existe, sinon
/// « introuvable » (pas encore fabriquée, média inconnu).
#[must_use]
pub fn thumbnail_response(cache: Option<&ThumbnailCache>, media_id: MediaId) -> Response<Vec<u8>> {
    let Some(cache) = cache else {
        return status(StatusCode::NOT_FOUND);
    };
    match std::fs::read(cache.path_for(media_id)) {
        Ok(bytes) => image(bytes, "image/jpeg"),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

/// Réponse à une demande d'aperçu : fabriqué au premier affichage, puis lu
/// dans le cache (même si le fichier d'origine est devenu introuvable).
#[must_use]
pub fn preview_response(
    previews: &PreviewCache,
    media_id: MediaId,
    file: &MediaFile,
) -> Response<Vec<u8>> {
    let read = previews
        .get_or_create(media_id, &file.path, file.format)
        .map_err(|error| failure_status(&error, &file.path))
        .and_then(|path| std::fs::read(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR));
    match read {
        Ok(bytes) => image(bytes, "image/jpeg"),
        Err(code) => status(code),
    }
}

/// Réponse à une demande de pleine résolution : le fichier d'origine tel
/// quel si l'interface sait le lire, sinon une conversion en JPEG faite en
/// mémoire (RAW) ; pour une vidéo, son aperçu (pas de lecture vidéo en
/// version 1).
#[must_use]
pub fn original_response(
    previews: &PreviewCache,
    media_id: MediaId,
    file: &MediaFile,
) -> Response<Vec<u8>> {
    if file.format.kind() == MediaKind::Video {
        return preview_response(previews, media_id, file);
    }
    if let Some(content_type) = displayable_type(file.format) {
        return match std::fs::read(&file.path) {
            Ok(bytes) => image(bytes, content_type),
            Err(error) => {
                tracing::warn!(%error, path = %file.path.display(), "cannot read an original");
                status(StatusCode::NOT_FOUND)
            }
        };
    }
    let request = FullSizeRequest {
        source: &file.path,
        prefer_embedded_preview: file.format.is_raw(),
        // Un aperçu intégré plus petit que l'aperçu de la Loupe
        // n'apporterait rien au zoom : la photo est alors décodée.
        min_embedded_preview_size: PREVIEW_SIZE,
        jpeg_quality: FULL_SIZE_JPEG_QUALITY,
        engine: platform::preferred_engine(file.format, ImagePurpose::FullSize),
    };
    match platform::make_full_size_jpeg(&request) {
        Ok(full_size) => image(full_size.jpeg, "image/jpeg"),
        Err(error) => {
            tracing::warn!(%error, path = %file.path.display(), "cannot convert an original");
            status(StatusCode::UNPROCESSABLE_ENTITY)
        }
    }
}

/// Type des formats que la vue web de macOS affiche directement.
///
/// À revoir pour Windows : WebView2 ne lit ni le HEIC ni le TIFF.
fn displayable_type(format: MediaFormat) -> Option<&'static str> {
    match format {
        MediaFormat::Jpeg => Some("image/jpeg"),
        MediaFormat::Png => Some("image/png"),
        MediaFormat::Heic => Some("image/heic"),
        MediaFormat::Tiff => Some("image/tiff"),
        _ => None,
    }
}

/// Code de réponse d'un échec de fabrication d'aperçu : fichier
/// introuvable, illisible, ou problème du cache.
fn failure_status(error: &CacheError, source: &Path) -> StatusCode {
    if !source.is_file() {
        tracing::info!(path = %source.display(), "cannot make a preview: file missing");
        StatusCode::NOT_FOUND
    } else if matches!(error, CacheError::Thumbnail(ThumbnailError::Undecodable)) {
        tracing::warn!(%error, "cannot make a preview");
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        tracing::error!(%error, "cannot make a preview");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Image demandée, si l'adresse est bien `…/<type>/<numéro>`. Seuls des
/// chiffres sont acceptés : aucun chemin ne peut être glissé dans l'adresse.
#[must_use]
pub fn requested_resource(uri: &Uri) -> Option<Resource> {
    // Seul le numéro de version de l'image est permis (`?v=<chiffres>`) : il ne sert qu'à contourner la mémoire de la vue web et
    // n'est pas lu. Tout autre paramètre fait refuser l'adresse.
    if let Some(query) = uri.query()
        && query.strip_prefix("v=").and_then(number).is_none()
    {
        return None;
    }
    let host = uri.host().unwrap_or_default();
    let segments: Vec<&str> = std::iter::once(host)
        .chain(uri.path().split('/'))
        .filter(|segment| !segment.is_empty())
        .collect();
    if let Some(position) = segments.iter().position(|segment| *segment == "tile") {
        let [_, z, x, y] = &segments[position..] else {
            return None;
        };
        return TileCoord::new(
            number(z)?.try_into().ok()?,
            number(x)?.try_into().ok()?,
            number(y)?.try_into().ok()?,
        )
        .map(Resource::MapTile);
    }
    if let Some(position) = segments.iter().position(|segment| *segment == "import") {
        let [_, analysis, index] = &segments[position..] else {
            return None;
        };
        return Some(Resource::ImportThumbnail {
            analysis: number(analysis)?.try_into().ok()?,
            index: number(index)?.try_into().ok()?,
        });
    }
    let position = segments
        .iter()
        .position(|segment| matches!(*segment, "thumbnail" | "preview" | "original"))?;
    let [kind, id] = &segments[position..] else {
        return None;
    };
    let media_id = media_id(id)?;
    match *kind {
        "thumbnail" => Some(Resource::Thumbnail(media_id)),
        "preview" => Some(Resource::Preview(media_id)),
        _ => Some(Resource::Original(media_id)),
    }
}

fn media_id(text: &str) -> Option<MediaId> {
    number(text)
        .and_then(|value| i64::try_from(value).ok())
        .filter(|value| *value > 0)
        .map(MediaId::new)
}

/// Nombre fait seulement de chiffres.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn image(bytes: Vec<u8>, content_type: &'static str) -> Response<Vec<u8>> {
    let mut response = Response::new(bytes);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    // Le navigateur peut garder l'image en mémoire un moment : elle ne change
    // qu'en cas de nouvelle fabrication.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    response
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}
