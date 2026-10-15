-- Migration 4 : attributs de tri.
--
-- Les médias déjà présents n'ont ni marqueur, ni note, ni couleur.

-- Marqueur : « picked » (retenue), « rejected » (rejetée), NULL (aucun).
ALTER TABLE media ADD COLUMN flag TEXT CHECK (flag IN ('picked', 'rejected'));
-- Note de 0 (sans note) à 5 étoiles.
ALTER TABLE media ADD COLUMN rating INTEGER NOT NULL DEFAULT 0
    CHECK (rating BETWEEN 0 AND 5);
-- Étiquette de couleur, NULL si aucune.
ALTER TABLE media ADD COLUMN color_label TEXT
    CHECK (color_label IN ('red', 'yellow', 'green', 'blue', 'purple'));
