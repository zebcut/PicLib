-- Migration 8 : lieux remplis automatiquement depuis la position GPS.
--
-- Une ligne par média déjà traité, même si aucun lieu n'a été trouvé (en
-- pleine mer...) : un champ que l'utilisateur a vidé n'est jamais rempli de
-- nouveau. Les colonnes gardent la valeur que PicLib a lui-même écrite
-- (NULL : rien écrit, le champ était déjà rempli ou aucun lieu n'a été
-- trouvé). On sait ainsi, plus tard, distinguer une saisie de l'utilisateur
-- d'une valeur automatique, par exemple pour mettre le lieu à jour quand la
-- position GPS change sans jamais écraser ce qu'il a tapé.
CREATE TABLE place_lookups (
    media_id INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    country  TEXT,
    region   TEXT,
    city     TEXT
) STRICT;
