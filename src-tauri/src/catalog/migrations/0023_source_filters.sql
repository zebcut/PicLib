-- Migration 23 : filtre mémorisé pour chaque source, quand l'option « Mémoriser un filtre pour chaque dossier ou
-- mot-clé » est cochée.
--
-- Une ligne par source : tout le catalogue, le dernier import, un mot-clé ou
-- un dossier. Le filtre d'un mot-clé ou d'un dossier disparaît avec lui.
-- C'est un réglage d'affichage, hors de l'annulation (Cmd+Z).
CREATE TABLE source_filters (
    kind       TEXT    NOT NULL CHECK (kind IN ('all', 'latestImport', 'keyword', 'folder')),
    keyword_id INTEGER REFERENCES keywords (id) ON DELETE CASCADE,
    folder_id  INTEGER REFERENCES folders (id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    CHECK ((kind = 'keyword') = (keyword_id IS NOT NULL)),
    CHECK ((kind = 'folder') = (folder_id IS NOT NULL))
) STRICT;

CREATE UNIQUE INDEX source_filters_unique
    ON source_filters (kind, coalesce(keyword_id, 0), coalesce(folder_id, 0));
CREATE INDEX source_filters_by_keyword ON source_filters (keyword_id);
CREATE INDEX source_filters_by_folder ON source_filters (folder_id);
