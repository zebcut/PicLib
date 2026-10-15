-- Migration 3 : suivi des vignettes.
--
-- Une ligne par média dont la vignette a été fabriquée (ou n'a pas pu
-- l'être). Un média sans ligne attend sa vignette. Le fichier lui-même est
-- dans le dossier « Vignettes » du dossier de travail.

CREATE TABLE thumbnails (
    media_id       INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    status         TEXT    NOT NULL CHECK (status IN ('ready', 'failed')),
    width          INTEGER CHECK (width > 0),
    height         INTEGER CHECK (height > 0),
    -- Aperçu intégré au fichier (RAW) ou image décodée.
    source         TEXT    CHECK (source IN ('embedded', 'decoded')),
    generated_at   INTEGER NOT NULL,
    -- Date de modification du fichier d'origine au moment de la fabrication :
    -- permettra de repérer une photo modifiée depuis.
    file_modified_at INTEGER NOT NULL
) STRICT;
