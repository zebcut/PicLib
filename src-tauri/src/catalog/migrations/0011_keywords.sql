-- Migration 11 : mots-clés hiérarchiques.
--
-- Un mot-clé est identifié par son parent et son nom, jamais par son nom
-- seul : « Orange » sous Fruits et sous Couleurs sont deux mots-clés.
-- `name_key` (forme Unicode composée, minuscules) rend « Paris » et « paris »
-- identiques ; `search_key` (sans accents ni majuscules) sert à
-- l'autocomplétion.
CREATE TABLE keywords (
    id                INTEGER PRIMARY KEY,
    parent_id         INTEGER REFERENCES keywords (id) ON DELETE RESTRICT,
    name              TEXT NOT NULL CHECK (name <> ''),
    name_key          TEXT NOT NULL,
    search_key        TEXT NOT NULL,
    -- « Inclure à l'export » : faux pour un mot-clé qui ne sert
    -- qu'à ranger (« Lieux »).
    include_on_export INTEGER NOT NULL DEFAULT 1 CHECK (include_on_export IN (0, 1))
) STRICT;

-- Unicité (parent, nom) ; `coalesce` : les mots-clés de la racine (parent
-- NULL) doivent aussi être uniques entre eux.
CREATE UNIQUE INDEX keywords_unique_name ON keywords (coalesce(parent_id, 0), name_key);
CREATE INDEX keywords_by_name_key ON keywords (name_key);

-- Mots-clés saisis sur chaque média. Les parents ne sont jamais stockés ici :
-- ils se déduisent de l'arbre(« parents impliqués »).
CREATE TABLE media_keywords (
    media_id   INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    keyword_id INTEGER NOT NULL REFERENCES keywords (id) ON DELETE CASCADE,
    PRIMARY KEY (media_id, keyword_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX media_keywords_by_keyword ON media_keywords (keyword_id, media_id);
