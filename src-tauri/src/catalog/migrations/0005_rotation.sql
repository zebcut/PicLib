-- Migration 5 : rotation choisie par l'utilisateur.
--
-- En degrés dans le sens horaire, en plus de l'orientation EXIF d'origine.
-- Elle ne s'applique qu'à l'affichage : le fichier n'est jamais modifié.
ALTER TABLE media ADD COLUMN rotation INTEGER NOT NULL DEFAULT 0
    CHECK (rotation IN (0, 90, 180, 270));
