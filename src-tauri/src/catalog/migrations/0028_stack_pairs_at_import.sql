-- Migration 28 : empiler les paires RAW + JPEG à l'import, un choix de la fenêtre d'import retenu pour le suivant,
-- coché par défaut.
ALTER TABLE import_preferences
    ADD COLUMN stack_pairs INTEGER NOT NULL DEFAULT 1 CHECK (stack_pairs IN (0, 1));
