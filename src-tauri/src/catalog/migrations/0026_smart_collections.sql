-- Migration 26 : collections dynamiques.
--
-- Une collection dynamique est une collection de l'arbre (migration 24) qui
-- a des règles, en JSON, vérifiées à la lecture ; ses photos ne sont pas
-- rangées : elles sont recalculées à chaque lecture.
CREATE TABLE smart_collections (
    collection_id INTEGER PRIMARY KEY REFERENCES collections (id) ON DELETE CASCADE,
    rules         TEXT    NOT NULL
) STRICT;
