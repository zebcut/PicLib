-- Migration 29 : empiler les rafales à l'import,
-- un choix de la fenêtre d'import retenu pour le suivant, décoché par
-- défaut, avec l'écart maximal entre deux photos (2 s par défaut).
ALTER TABLE import_preferences
    ADD COLUMN stack_bursts INTEGER NOT NULL DEFAULT 0 CHECK (stack_bursts IN (0, 1));
ALTER TABLE import_preferences
    ADD COLUMN burst_gap_seconds INTEGER NOT NULL DEFAULT 2
        CHECK (burst_gap_seconds BETWEEN 1 AND 60);
