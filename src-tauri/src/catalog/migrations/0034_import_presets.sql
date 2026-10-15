-- Migration 34 : préréglages d'import : tous les
-- réglages de l'écran d'import sous un nom. Le contenu est en JSON,
-- vérifié à la lecture.
CREATE TABLE import_presets (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL,
    name_key TEXT    NOT NULL UNIQUE,
    content  TEXT    NOT NULL
) STRICT;
