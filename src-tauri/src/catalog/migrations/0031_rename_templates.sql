-- Migration 31 : modèles de renommage enregistrés. Le contenu est en JSON, vérifié à la lecture.
CREATE TABLE rename_templates (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL,
    name_key TEXT    NOT NULL UNIQUE,
    content  TEXT    NOT NULL
) STRICT;
