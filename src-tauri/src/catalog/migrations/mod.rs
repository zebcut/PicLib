//! Migrations versionnées du catalogue.
//!
//! Une migration est un script SQL numéroté qui fait passer la structure de la
//! base d'une version à la suivante. La version actuelle est enregistrée dans
//! la base elle-même (`PRAGMA user_version`). Les migrations ne sont jamais
//! modifiées une fois publiées : on en ajoute une nouvelle.

use rusqlite::Connection;

/// Une étape d'évolution de la structure du catalogue.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Version atteinte après cette migration (1, 2, 3...).
    pub version: u32,
    /// Courte description, pour le journal.
    pub description: &'static str,
    /// Script SQL exécuté.
    pub sql: &'static str,
}

/// Toutes les migrations, dans l'ordre. Les versions se suivent sans trou à
/// partir de 1 (vérifié par un test).
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema",
        sql: include_str!("0001_initial.sql"),
    },
    Migration {
        version: 2,
        description: "media metadata and import tracking",
        sql: include_str!("0002_import_metadata.sql"),
    },
    Migration {
        version: 3,
        description: "thumbnails",
        sql: include_str!("0003_thumbnails.sql"),
    },
    Migration {
        version: 4,
        description: "flags, ratings and color labels",
        sql: include_str!("0004_triage.sql"),
    },
    Migration {
        version: 5,
        description: "user rotation",
        sql: include_str!("0005_rotation.sql"),
    },
    Migration {
        version: 6,
        description: "id watermark and file operations journal",
        sql: include_str!("0006_file_operations.sql"),
    },
    Migration {
        version: 7,
        description: "descriptive metadata",
        sql: include_str!("0007_descriptive_metadata.sql"),
    },
    Migration {
        version: 8,
        description: "places filled from GPS positions",
        sql: include_str!("0008_place_lookups.sql"),
    },
    Migration {
        version: 9,
        description: "discard null GPS positions",
        sql: include_str!("0009_discard_null_gps.sql"),
    },
    Migration {
        version: 10,
        description: "original capture time",
        sql: include_str!("0010_original_capture_time.sql"),
    },
    Migration {
        version: 11,
        description: "hierarchical keywords",
        sql: include_str!("0011_keywords.sql"),
    },
    Migration {
        version: 12,
        description: "keyword id watermark",
        sql: include_str!("0012_keyword_id_watermark.sql"),
    },
    Migration {
        version: 13,
        description: "keyword sets",
        sql: include_str!("0013_keyword_sets.sql"),
    },
    Migration {
        version: 14,
        description: "keyword last use",
        sql: include_str!("0014_keyword_last_used.sql"),
    },
    Migration {
        version: 15,
        description: "metadata templates",
        sql: include_str!("0015_metadata_templates.sql"),
    },
    Migration {
        version: 16,
        description: "import template",
        sql: include_str!("0016_import_template.sql"),
    },
    Migration {
        version: 17,
        description: "recovered metadata counts",
        sql: include_str!("0017_recovered_metadata_counts.sql"),
    },
    Migration {
        version: 18,
        description: "copy import",
        sql: include_str!("0018_copy_import.sql"),
    },
    Migration {
        version: 19,
        description: "journal intents",
        sql: include_str!("0019_journal_intents.sql"),
    },
    Migration {
        version: 20,
        description: "folder moves",
        sql: include_str!("0020_folder_moves.sql"),
    },
    Migration {
        version: 21,
        description: "place keywords",
        sql: include_str!("0021_place_keywords.sql"),
    },
    Migration {
        version: 22,
        description: "filter presets",
        sql: include_str!("0022_filter_presets.sql"),
    },
    Migration {
        version: 23,
        description: "source filters",
        sql: include_str!("0023_source_filters.sql"),
    },
    Migration {
        version: 24,
        description: "collections",
        sql: include_str!("0024_collections.sql"),
    },
    Migration {
        version: 25,
        description: "quick collection",
        sql: include_str!("0025_quick_collection.sql"),
    },
    Migration {
        version: 26,
        description: "smart collections",
        sql: include_str!("0026_smart_collections.sql"),
    },
    Migration {
        version: 27,
        description: "stacks",
        sql: include_str!("0027_stacks.sql"),
    },
    Migration {
        version: 28,
        description: "stack RAW + JPEG pairs at import",
        sql: include_str!("0028_stack_pairs_at_import.sql"),
    },
    Migration {
        version: 29,
        description: "stack bursts at import",
        sql: include_str!("0029_stack_bursts_at_import.sql"),
    },
    Migration {
        version: 30,
        description: "missing files",
        sql: include_str!("0030_missing_files.sql"),
    },
    Migration {
        version: 31,
        description: "rename templates",
        sql: include_str!("0031_rename_templates.sql"),
    },
    Migration {
        version: 32,
        description: "import layout",
        sql: include_str!("0032_import_layout.sql"),
    },
    Migration {
        version: 33,
        description: "import backup",
        sql: include_str!("0033_import_backup.sql"),
    },
    Migration {
        version: 34,
        description: "import presets",
        sql: include_str!("0034_import_presets.sql"),
    },
    Migration {
        version: 35,
        description: "recent import folders",
        sql: include_str!("0035_recent_import_folders.sql"),
    },
    Migration {
        version: 36,
        description: "xmp sync",
        sql: include_str!("0036_xmp_sync.sql"),
    },
    Migration {
        version: 37,
        description: "external editing",
        sql: include_str!("0037_external_editing.sql"),
    },
];

/// Version de structure la plus récente connue de cette version de PicLib.
#[must_use]
pub fn latest_version(migrations: &[Migration]) -> u32 {
    migrations.last().map_or(0, |migration| migration.version)
}

/// Migrations restant à appliquer à une base en version `current_version`.
#[must_use]
pub fn pending(migrations: &[Migration], current_version: u32) -> &[Migration] {
    let first_pending = migrations
        .iter()
        .position(|migration| migration.version > current_version)
        .unwrap_or(migrations.len());
    &migrations[first_pending..]
}

/// Lit la version de structure de la base.
///
/// # Errors
///
/// Renvoie l'erreur SQLite si la base est illisible.
pub fn current_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Applique une migration dans une transaction : soit elle réussit
/// entièrement (structure et numéro de version), soit la base reste
/// exactement dans son état précédent.
///
/// # Errors
///
/// Renvoie l'erreur SQLite qui a interrompu la migration.
pub fn apply(connection: &mut Connection, migration: &Migration) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(migration.sql)?;
    transaction.pragma_update(None, "user_version", migration.version)?;
    transaction.commit()
}
