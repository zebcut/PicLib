-- Migration 30 : fichiers manquants.
--
-- `file_missing` vaut 1 quand la vérification de fond n'a pas trouvé le
-- fichier (déplacé, supprimé hors de PicLib, disque débranché) ; elle le
-- remet à 0 quand il revient. L'index ne contient que les manquants.
ALTER TABLE media
    ADD COLUMN file_missing INTEGER NOT NULL DEFAULT 0 CHECK (file_missing IN (0, 1));

CREATE INDEX media_missing ON media (id) WHERE file_missing = 1;

-- Le filtre mémorisé d'une source (migrations 23 à 25) vaut aussi pour
-- « Photos manquantes ».
CREATE TABLE source_filters_new (
    kind          TEXT    NOT NULL
        CHECK (kind IN ('all', 'latestImport', 'keyword', 'folder', 'collection',
                        'quickCollection', 'missing')),
    keyword_id    INTEGER REFERENCES keywords (id) ON DELETE CASCADE,
    folder_id     INTEGER REFERENCES folders (id) ON DELETE CASCADE,
    collection_id INTEGER REFERENCES collections (id) ON DELETE CASCADE,
    content       TEXT    NOT NULL,
    CHECK ((kind = 'keyword') = (keyword_id IS NOT NULL)),
    CHECK ((kind = 'folder') = (folder_id IS NOT NULL)),
    CHECK ((kind = 'collection') = (collection_id IS NOT NULL))
) STRICT;

INSERT INTO source_filters_new (kind, keyword_id, folder_id, collection_id, content)
    SELECT kind, keyword_id, folder_id, collection_id, content FROM source_filters;
DROP TABLE source_filters;
ALTER TABLE source_filters_new RENAME TO source_filters;

CREATE UNIQUE INDEX source_filters_unique ON source_filters
    (kind, coalesce(keyword_id, 0), coalesce(folder_id, 0), coalesce(collection_id, 0));
CREATE INDEX source_filters_by_keyword ON source_filters (keyword_id);
CREATE INDEX source_filters_by_folder ON source_filters (folder_id);
CREATE INDEX source_filters_by_collection ON source_filters (collection_id);
