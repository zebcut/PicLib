-- Migration 10 : date de prise de vue d'origine.
--
-- La date de prise de vue peut désormais être modifiée dans PicLib. La date
-- lue à l'import est gardée à part, jamais modifiée : on peut y revenir, et
-- c'est elle que compare la détection des doublons. Sinon,
-- réimporter une photo dont on a corrigé la date l'ajouterait une seconde
-- fois.
ALTER TABLE media ADD COLUMN original_capture_time TEXT NOT NULL DEFAULT '';
ALTER TABLE media ADD COLUMN original_capture_offset_seconds INTEGER;

UPDATE media
SET original_capture_time = capture_time,
    original_capture_offset_seconds = capture_offset_seconds;

DROP INDEX media_duplicates;
CREATE INDEX media_duplicates ON media (name_key, file_size, original_capture_time);
