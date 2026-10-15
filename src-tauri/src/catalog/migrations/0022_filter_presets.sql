-- Migration 22 : préréglages de filtres.
--
-- Un préréglage a un nom unique (sans tenir compte des majuscules) et un
-- contenu : les onglets ouverts, les critères des filtres et les colonnes du
-- filtre Métadonnées, en JSON, vérifié à la lecture. Les préréglages
-- fournis ne sont pas dans la base : l'interface les connaît.
CREATE TABLE filter_presets (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL CHECK (name <> ''),
    name_key TEXT    NOT NULL UNIQUE,
    content  TEXT    NOT NULL
) STRICT;
