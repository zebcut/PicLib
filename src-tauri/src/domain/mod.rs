//! Types métier purs de PicLib.
//!
//! Ce module ne dépend ni de Tauri, ni de la base de données, ni de
//! l'interface : il décrit les notions de l'application (dossier, média,
//! import...) et rien d'autre.

mod capture_time;
mod folder;
mod ids;
mod import;
mod keyword;
mod media;
mod media_details;
mod media_format;
mod metadata;
mod place;
mod rotation;
mod text_search;
mod timestamp;
mod triage;

pub use capture_time::{CaptureTime, CaptureTimeSource};
pub use folder::Folder;
pub use ids::{
    CollectionId, FilterPresetId, FolderId, ImportId, KeywordId, KeywordSetId, MediaId,
    MetadataTemplateId, StackId,
};
pub use import::{Import, ImportCounts, ImportMode, ImportStatus};
pub use keyword::{
    KeywordEntryError, MAX_KEYWORD_CHARS, keyword_name_key, keyword_search_key,
    parse_keyword_entry, parse_keyword_name, recovered_keyword_name,
};
pub use media::{Media, MediaKind, NewMedia, duplicate_name_key};
pub use media_details::{GpsPosition, MediaDetails, Orientation};
pub use media_format::MediaFormat;
pub use metadata::{MAX_METADATA_CHARS, MetadataField, metadata_text};
pub use place::{Language, PlaceNames};
pub use rotation::{Rotation, RotationDirection};
pub use text_search::{TextField, TextRule, TextTerms, search_key};
pub use timestamp::Timestamp;
pub use triage::{ColorLabel, Flag, Rating};
