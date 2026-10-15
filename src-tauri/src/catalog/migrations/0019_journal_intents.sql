-- Migration 19 : journal d'intention.
--
-- Chaque fichier copié ou mis à la corbeille est noté « en attente »
-- (`pending`) AVANT l'opération sur le disque, puis « fait » dans la même
-- transaction que son inscription au catalogue (ou son retrait). Après un
-- arrêt brutal, les lignes restées en attente disent ce qu'il reste à
-- terminer à l'ouverture suivante.
--
-- Pour une copie en attente :
-- - `content_hash` vide : la copie n'est pas encore vérifiée ;
--   `destination_path` est le dossier où elle s'écrit (fichier provisoire
--   caché) ;
-- - `content_hash` rempli : la copie est vérifiée ; `destination_path` est
--   son nom définitif.
--
-- SQLite ne sait pas modifier une contrainte CHECK : la table des fichiers
-- du journal est refaite à l'identique, avec l'état `pending` et
-- l'empreinte.
CREATE TABLE file_operation_items_v19 (
    id           INTEGER PRIMARY KEY,
    operation_id INTEGER NOT NULL REFERENCES file_operations (id) ON DELETE CASCADE,
    source_path  TEXT    NOT NULL,
    -- Emplacement du fichier après l'opération (dans la corbeille, copie),
    -- s'il est connu ; pour une copie en attente, voir plus haut.
    destination_path TEXT,
    result       TEXT    NOT NULL
        CHECK (result IN ('pending', 'done', 'missing', 'failed')),
    detail       TEXT,
    -- Empreinte SHA-256 d'une copie vérifiée.
    content_hash TEXT CHECK (content_hash IS NULL OR length(content_hash) = 64)
) STRICT;

INSERT INTO file_operation_items_v19
        (id, operation_id, source_path, destination_path, result, detail)
    SELECT id, operation_id, source_path, destination_path, result, detail
    FROM file_operation_items;

DROP TABLE file_operation_items;
ALTER TABLE file_operation_items_v19 RENAME TO file_operation_items;
CREATE INDEX file_operation_items_by_operation ON file_operation_items (operation_id);
CREATE INDEX file_operation_items_pending ON file_operation_items (operation_id)
    WHERE result = 'pending';

-- Import dont une opération de copie fait partie : les copies terminées
-- après un arrêt brutal lui sont rattachées.
ALTER TABLE file_operations ADD COLUMN import_id INTEGER REFERENCES imports (id);
