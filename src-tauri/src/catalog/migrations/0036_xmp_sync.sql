-- Migration 36 : suivi de l'écriture des fichiers annexes XMP.
--
-- Une ligne par média dont les métadonnées ont changé depuis l'ouverture de
-- cette version, ou dont PicLib connaît le fichier XMP. Pas de ligne : rien
-- à écrire, aucun fichier connu (les médias déjà au catalogue partent ainsi
-- « à jour » : seule une modification les rend « à enregistrer »).
--
-- `revision` augmente à chaque modification d'une information écrite dans
-- le XMP (déclencheurs ci-dessous) ; `written_revision` est la révision
-- écrite dans le fichier. Un média est « à enregistrer » quand elles
-- diffèrent. Une écriture note la révision lue avant d'écrire : une
-- modification faite pendant l'écriture n'est pas perdue.
CREATE TABLE xmp_sync (
    media_id          INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    revision          INTEGER NOT NULL DEFAULT 0,
    written_revision  INTEGER NOT NULL DEFAULT 0,
    -- Fichier XMP tel que PicLib l'a écrit ou lu en dernier : son nom (dans
    -- le dossier de la photo), sa date (millisecondes) et sa taille, et si
    -- PicLib l'a écrit (1) ou seulement lu (0) ; NULL : aucun fichier connu.
    sidecar_name      TEXT,
    file_modified_at  INTEGER,
    file_size         INTEGER CHECK (file_size IS NULL OR file_size >= 0),
    file_written      INTEGER NOT NULL DEFAULT 0 CHECK (file_written IN (0, 1)),
    -- Le fichier a été modifié par un autre logiciel.
    changed_elsewhere INTEGER NOT NULL DEFAULT 0 CHECK (changed_elsewhere IN (0, 1)),
    -- Dernière écriture échouée : raison (effacée à la modification suivante).
    error             TEXT CHECK (error IN (
        'missingPhoto', 'cameraCard', 'unreadableSidecar', 'notPreserved', 'writeFailed'
    ))
) STRICT;

CREATE INDEX xmp_sync_pending ON xmp_sync (media_id) WHERE revision <> written_revision;

-- Les colonnes écrites dans le XMP. Une ligne absente est créée ; `error`
-- est effacée : la modification suivante retente l'écriture. Une seule
-- instruction (« upsert ») par ligne : mesuré à 130 ms de plus pour une
-- note donnée à 50 000 photos d'un coup, contre 220 ms avec deux
-- instructions.
CREATE TRIGGER xmp_media_changed
AFTER UPDATE OF flag, rating, color_label, rotation, title, caption, creator, copyright,
    country, region, city, location, gps_latitude, gps_longitude, gps_altitude,
    capture_time, capture_offset_seconds, orientation ON media
WHEN OLD.flag IS NOT NEW.flag OR OLD.rating IS NOT NEW.rating
    OR OLD.color_label IS NOT NEW.color_label OR OLD.rotation IS NOT NEW.rotation
    OR OLD.title IS NOT NEW.title OR OLD.caption IS NOT NEW.caption
    OR OLD.creator IS NOT NEW.creator OR OLD.copyright IS NOT NEW.copyright
    OR OLD.country IS NOT NEW.country OR OLD.region IS NOT NEW.region
    OR OLD.city IS NOT NEW.city OR OLD.location IS NOT NEW.location
    OR OLD.gps_latitude IS NOT NEW.gps_latitude OR OLD.gps_longitude IS NOT NEW.gps_longitude
    OR OLD.gps_altitude IS NOT NEW.gps_altitude OR OLD.capture_time IS NOT NEW.capture_time
    OR OLD.capture_offset_seconds IS NOT NEW.capture_offset_seconds
    OR OLD.orientation IS NOT NEW.orientation
BEGIN
    INSERT INTO xmp_sync (media_id, revision) VALUES (NEW.id, 1)
        ON CONFLICT (media_id) DO UPDATE SET revision = revision + 1, error = NULL;
END;

-- Mots-clés posés ou retirés. Un média qui quitte le catalogue retire ses
-- mots-clés en cascade : il n'a plus de ligne à créer.
CREATE TRIGGER xmp_keyword_added
AFTER INSERT ON media_keywords
BEGIN
    INSERT INTO xmp_sync (media_id, revision) VALUES (NEW.media_id, 1)
        ON CONFLICT (media_id) DO UPDATE SET revision = revision + 1, error = NULL;
END;

CREATE TRIGGER xmp_keyword_removed
AFTER DELETE ON media_keywords
WHEN EXISTS (SELECT 1 FROM media WHERE id = OLD.media_id)
BEGIN
    INSERT INTO xmp_sync (media_id, revision) VALUES (OLD.media_id, 1)
        ON CONFLICT (media_id) DO UPDATE SET revision = revision + 1, error = NULL;
END;
