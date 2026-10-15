-- Migration 21 : mots-clés de lieu.
--
-- Une seule ligne : le mot-clé « Lieux » sous lequel PicLib range les
-- mots-clés tirés des champs Pays, Région, Ville et Lieu précis (retenu
-- par son identifiant : l'utilisateur peut le renommer), et le rattrapage
-- des photos déjà au catalogue, fait une seule fois.
CREATE TABLE place_keywords (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    root_keyword_id INTEGER REFERENCES keywords (id) ON DELETE SET NULL,
    backfilled      INTEGER NOT NULL DEFAULT 0 CHECK (backfilled IN (0, 1))
) STRICT;

INSERT INTO place_keywords (id) VALUES (1);
