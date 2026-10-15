-- Migration 35 : sources de l'écran d'import : les derniers
-- dossiers choisis, retenus d'une session à l'autre, et le choix d'inclure
-- les sous-dossiers (coché par défaut).
CREATE TABLE recent_import_folders (
    path    TEXT    NOT NULL PRIMARY KEY,
    used_at INTEGER NOT NULL
) STRICT;

ALTER TABLE import_preferences
    ADD COLUMN include_subfolders INTEGER NOT NULL DEFAULT 1
        CHECK (include_subfolders IN (0, 1));
