-- Migration 24 : collections manuelles et dossiers de collections.
--
-- Dossiers et collections forment un même arbre : seul un dossier a des
-- enfants (vérifié par PicLib), seule une collection a des photos. Un nom
-- est unique parmi ses voisins, sans tenir compte des majuscules.
CREATE TABLE collections (
    id        INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES collections (id) ON DELETE RESTRICT,
    kind      TEXT    NOT NULL CHECK (kind IN ('collection', 'folder')),
    name      TEXT    NOT NULL CHECK (name <> ''),
    name_key  TEXT    NOT NULL
) STRICT;

CREATE UNIQUE INDEX collections_unique_name ON collections (coalesce(parent_id, 0), name_key);
CREATE INDEX collections_by_parent ON collections (parent_id);

-- Une photo peut être dans plusieurs collections sans être dupliquée ; la
-- retirer d'une collection ne la retire pas du catalogue.
CREATE TABLE collection_media (
    collection_id INTEGER NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    media_id      INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, media_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX collection_media_by_media ON collection_media (media_id, collection_id);

-- Le filtre mémorisé d'une source (migration 23) vaut aussi pour une
-- collection ou un dossier de collections.
CREATE TABLE source_filters_new (
    kind          TEXT    NOT NULL
        CHECK (kind IN ('all', 'latestImport', 'keyword', 'folder', 'collection')),
    keyword_id    INTEGER REFERENCES keywords (id) ON DELETE CASCADE,
    folder_id     INTEGER REFERENCES folders (id) ON DELETE CASCADE,
    collection_id INTEGER REFERENCES collections (id) ON DELETE CASCADE,
    content       TEXT    NOT NULL,
    CHECK ((kind = 'keyword') = (keyword_id IS NOT NULL)),
    CHECK ((kind = 'folder') = (folder_id IS NOT NULL)),
    CHECK ((kind = 'collection') = (collection_id IS NOT NULL))
) STRICT;

INSERT INTO source_filters_new (kind, keyword_id, folder_id, content)
    SELECT kind, keyword_id, folder_id, content FROM source_filters;
DROP TABLE source_filters;
ALTER TABLE source_filters_new RENAME TO source_filters;

CREATE UNIQUE INDEX source_filters_unique ON source_filters
    (kind, coalesce(keyword_id, 0), coalesce(folder_id, 0), coalesce(collection_id, 0));
CREATE INDEX source_filters_by_keyword ON source_filters (keyword_id);
CREATE INDEX source_filters_by_folder ON source_filters (folder_id);
CREATE INDEX source_filters_by_collection ON source_filters (collection_id);
