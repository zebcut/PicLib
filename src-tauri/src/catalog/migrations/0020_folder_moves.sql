-- Migration 20 : le journal des opérations note aussi les déplacements de
-- dossiers.
--
-- SQLite ne sait pas modifier une contrainte CHECK : les deux tables du
-- journal sont refaites à l'identique, avec la nouvelle nature
-- d'opération. La table des fichiers est vidée avant sa table
-- d'opérations, pour qu'aucune suppression en cascade ne touche les lignes
-- recopiées.
CREATE TABLE file_operations_v20 (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL CHECK (kind IN ('trash', 'copy', 'move')),
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    status      TEXT    NOT NULL
        CHECK (status IN ('running', 'completed', 'cancelled', 'failed')),
    import_id   INTEGER REFERENCES imports (id)
) STRICT;

CREATE TABLE file_operation_items_v20 (
    id           INTEGER PRIMARY KEY,
    operation_id INTEGER NOT NULL REFERENCES file_operations_v20 (id) ON DELETE CASCADE,
    source_path  TEXT    NOT NULL,
    -- Emplacement après l'opération (dans la corbeille, copie, dossier
    -- déplacé), s'il est connu ; pour une copie en attente, voir la
    -- migration 19.
    destination_path TEXT,
    result       TEXT    NOT NULL
        CHECK (result IN ('pending', 'done', 'missing', 'failed')),
    detail       TEXT,
    content_hash TEXT CHECK (content_hash IS NULL OR length(content_hash) = 64)
) STRICT;

INSERT INTO file_operations_v20 (id, kind, started_at, finished_at, status, import_id)
    SELECT id, kind, started_at, finished_at, status, import_id FROM file_operations;
INSERT INTO file_operation_items_v20
        (id, operation_id, source_path, destination_path, result, detail, content_hash)
    SELECT id, operation_id, source_path, destination_path, result, detail, content_hash
    FROM file_operation_items;

DROP TABLE file_operation_items;
DROP TABLE file_operations;
ALTER TABLE file_operations_v20 RENAME TO file_operations;
ALTER TABLE file_operation_items_v20 RENAME TO file_operation_items;
CREATE INDEX file_operation_items_by_operation ON file_operation_items (operation_id);
CREATE INDEX file_operation_items_pending ON file_operation_items (operation_id)
    WHERE result = 'pending';
