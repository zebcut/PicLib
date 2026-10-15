-- Migration 1 : structure initiale du catalogue.
--
-- Tables « STRICT » : SQLite refuse une valeur du mauvais type (par défaut, il
-- accepterait par exemple du texte dans une colonne de nombres).
-- Les dates techniques sont en millisecondes depuis 1970 (UTC).

-- Marque le fichier comme catalogue PicLib (« PicL » en ASCII), pour ne
-- jamais confondre une autre base SQLite avec un catalogue.
PRAGMA application_id = 1349083980;

CREATE TABLE folders (
    id        INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES folders (id) ON DELETE RESTRICT,
    path      TEXT    NOT NULL UNIQUE
) STRICT;

CREATE INDEX folders_by_parent ON folders (parent_id);

CREATE TABLE imports (
    id          INTEGER PRIMARY KEY,
    mode        TEXT    NOT NULL CHECK (mode IN ('reference', 'copy')),
    source_path TEXT    NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER
) STRICT;

CREATE TABLE media (
    id               INTEGER PRIMARY KEY,
    folder_id        INTEGER NOT NULL REFERENCES folders (id) ON DELETE RESTRICT,
    file_name        TEXT    NOT NULL,
    kind             TEXT    NOT NULL CHECK (kind IN ('photo', 'video')),
    file_size        INTEGER NOT NULL CHECK (file_size >= 0),
    file_modified_at INTEGER NOT NULL,
    import_id        INTEGER NOT NULL REFERENCES imports (id) ON DELETE RESTRICT,
    UNIQUE (folder_id, file_name)
) STRICT;

CREATE INDEX media_by_import ON media (import_id);
