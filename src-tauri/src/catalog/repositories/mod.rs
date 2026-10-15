//! Dépôts : fonctions de lecture et d'écriture du catalogue, une par table.
//!
//! Elles reçoivent une `&Connection` : on peut leur passer
//! [`Catalog::connection`](super::Catalog::connection) ou une transaction
//! ([`Catalog::transaction`](super::Catalog::transaction)) pour regrouper
//! plusieurs écritures en une seule opération tout-ou-rien.

pub mod capture_time;
pub mod collections;
pub mod edited_versions;
pub mod file_operations;
pub mod filter_presets;
pub mod folders;
pub mod import_preferences;
pub mod import_presets;
pub mod imports;
pub mod keyword_changes;
pub mod keyword_sets;
pub mod keyword_usage;
pub mod keywords;
pub mod library;
mod library_query;
pub mod map;
pub mod media;
pub mod media_details;
pub mod metadata;
mod metadata_columns;
pub mod metadata_templates;
pub mod missing;
pub mod operations_log;
pub mod places;
pub mod quick_collection;
pub mod recent_import_folders;
pub mod removal;
pub mod rename_templates;
pub mod rotation;
pub mod smart_rules;
mod smart_sql;
pub mod source_filters;
pub mod stacks;
mod text_search;
pub mod thumbnails;
pub mod triage;
pub mod xmp_sync;
