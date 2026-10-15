-- Migration 6 : suppression de photos.

-- Plus grand identifiant jamais attribué à un média. SQLite réattribue
-- sinon le plus grand identifiant d'une ligne supprimée : une nouvelle photo
-- pourrait alors hériter de l'aperçu en cache d'une photo retirée, et
-- l'annulation d'un retrait ne pourrait plus remettre la photo à sa place.
CREATE TABLE id_watermarks (
    name    TEXT    PRIMARY KEY,
    last_id INTEGER NOT NULL CHECK (last_id >= 0)
) STRICT;

INSERT INTO id_watermarks (name, last_id)
    VALUES ('media', (SELECT coalesce(max(id), 0) FROM media));

-- Journal des opérations sur les fichiers : une ligne par
-- opération, une ligne par fichier concerné.
CREATE TABLE file_operations (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL CHECK (kind IN ('trash')),
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    status      TEXT    NOT NULL
        CHECK (status IN ('running', 'completed', 'cancelled', 'failed'))
) STRICT;

CREATE TABLE file_operation_items (
    id           INTEGER PRIMARY KEY,
    operation_id INTEGER NOT NULL REFERENCES file_operations (id) ON DELETE CASCADE,
    source_path  TEXT    NOT NULL,
    -- Emplacement du fichier après l'opération (dans la corbeille), s'il
    -- est connu.
    destination_path TEXT,
    result       TEXT    NOT NULL CHECK (result IN ('done', 'missing', 'failed')),
    detail       TEXT
) STRICT;

CREATE INDEX file_operation_items_by_operation ON file_operation_items (operation_id);
