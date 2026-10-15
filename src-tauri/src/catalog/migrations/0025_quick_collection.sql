-- Migration 25 : collection rapide et collection cible.
--
-- La collection rapide est unique et gardée d'une session à l'autre. La
-- collection cible reçoit les photos de la touche B ; sans cible (NULL),
-- c'est la collection rapide.
CREATE TABLE quick_collection (
    media_id INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE
) STRICT;

CREATE TABLE collection_target (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    collection_id INTEGER REFERENCES collections (id) ON DELETE SET NULL
) STRICT;

-- Le filtre mémorisé d'une source (migrations 23 et 24) vaut aussi pour la
-- collection rapide.
CREATE TABLE source_filters_new (
    kind          TEXT    NOT NULL
        CHECK (kind IN ('all', 'latestImport', 'keyword', 'folder', 'collection',
                        'quickCollection')),
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
