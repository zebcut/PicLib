-- Migration 16 : modèle de métadonnées choisi au dernier import,
-- présélectionné au suivant.
--
-- Au plus un modèle à la fois ; s'il est supprimé, l'import suivant repart
-- sans modèle.
ALTER TABLE metadata_templates
    ADD COLUMN used_at_import INTEGER NOT NULL DEFAULT 0 CHECK (used_at_import IN (0, 1));

CREATE UNIQUE INDEX metadata_templates_single_import ON metadata_templates (used_at_import)
    WHERE used_at_import = 1;
