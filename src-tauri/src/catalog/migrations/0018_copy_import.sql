-- Migration 18 : import par copie.

-- Empreinte SHA-256 du contenu (64 caractères hexadécimaux), connue pour
-- les fichiers importés par copie : une photo déjà importée sous un autre
-- nom est reconnue comme doublon. Vide pour l'import par
-- référence, qui ne lit pas les fichiers en entier.
ALTER TABLE media ADD COLUMN content_hash TEXT
    CHECK (content_hash IS NULL OR length(content_hash) = 64);
CREATE INDEX media_by_content_hash ON media (content_hash)
    WHERE content_hash IS NOT NULL;

-- Dossier où l'import par copie a rangé les fichiers, et fichiers qui
-- n'ont pas pu être copiés.
ALTER TABLE imports ADD COLUMN destination_path TEXT;
ALTER TABLE imports ADD COLUMN copy_failures INTEGER NOT NULL DEFAULT 0;

-- Le journal des opérations note aussi les copies. SQLite ne sait pas
-- modifier une contrainte CHECK : les deux tables du journal sont refaites
-- à l'identique, avec la nouvelle nature d'opération. La table des
-- fichiers est vidée avant sa table d'opérations, pour qu'aucune
-- suppression en cascade ne touche les lignes recopiées.
CREATE TABLE file_operations_v18 (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL CHECK (kind IN ('trash', 'copy')),
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    status      TEXT    NOT NULL
        CHECK (status IN ('running', 'completed', 'cancelled', 'failed'))
) STRICT;

CREATE TABLE file_operation_items_v18 (
    id           INTEGER PRIMARY KEY,
    operation_id INTEGER NOT NULL REFERENCES file_operations_v18 (id) ON DELETE CASCADE,
    source_path  TEXT    NOT NULL,
    -- Emplacement du fichier après l'opération (dans la corbeille, copie),
    -- s'il est connu.
    destination_path TEXT,
    result       TEXT    NOT NULL CHECK (result IN ('done', 'missing', 'failed')),
    detail       TEXT
) STRICT;

INSERT INTO file_operations_v18 (id, kind, started_at, finished_at, status)
    SELECT id, kind, started_at, finished_at, status FROM file_operations;
INSERT INTO file_operation_items_v18
        (id, operation_id, source_path, destination_path, result, detail)
    SELECT id, operation_id, source_path, destination_path, result, detail
    FROM file_operation_items;

DROP TABLE file_operation_items;
DROP TABLE file_operations;
ALTER TABLE file_operations_v18 RENAME TO file_operations;
ALTER TABLE file_operation_items_v18 RENAME TO file_operation_items;
CREATE INDEX file_operation_items_by_operation ON file_operation_items (operation_id);

-- Derniers choix de la fenêtre d'import, repris au suivant : une seule ligne. `destination` vide : le dossier `Media`
-- du dossier du catalogue.
CREATE TABLE import_preferences (
    id                       INTEGER PRIMARY KEY CHECK (id = 1),
    mode                     TEXT    NOT NULL CHECK (mode IN ('reference', 'copy')),
    destination              TEXT,
    original_path_in_caption INTEGER NOT NULL CHECK (original_path_in_caption IN (0, 1))
) STRICT;
