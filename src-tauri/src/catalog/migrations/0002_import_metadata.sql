-- Migration 2 : métadonnées des médias et suivi des imports.
--
-- Les colonnes ajoutées à une table existante doivent avoir une valeur par
-- défaut ; les médias déjà présents (impossible en pratique : l'import arrive
-- avec cette version) sont complétés plus bas à partir de ce qu'on sait.

-- Médias --------------------------------------------------------------------

-- Nom au moment de l'import, conservé si le fichier est renommé ensuite.
ALTER TABLE media ADD COLUMN original_file_name TEXT NOT NULL DEFAULT '';
-- Nom d'origine normalisé (Unicode NFC, minuscules) : clé des doublons.
ALTER TABLE media ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
ALTER TABLE media ADD COLUMN format TEXT NOT NULL DEFAULT '';
-- Date locale « 2024-07-14T18:30:05 » ; l'ordre du texte est l'ordre
-- chronologique.
ALTER TABLE media ADD COLUMN capture_time TEXT NOT NULL DEFAULT '';
-- Décalage par rapport à UTC en secondes, s'il est connu.
ALTER TABLE media ADD COLUMN capture_offset_seconds INTEGER;
ALTER TABLE media ADD COLUMN capture_time_source TEXT NOT NULL DEFAULT 'file'
    CHECK (capture_time_source IN ('exif', 'file'));
ALTER TABLE media ADD COLUMN width INTEGER CHECK (width > 0);
ALTER TABLE media ADD COLUMN height INTEGER CHECK (height > 0);
ALTER TABLE media ADD COLUMN orientation INTEGER CHECK (orientation BETWEEN 1 AND 8);
ALTER TABLE media ADD COLUMN camera_make TEXT;
ALTER TABLE media ADD COLUMN camera_model TEXT;
ALTER TABLE media ADD COLUMN lens TEXT;
ALTER TABLE media ADD COLUMN focal_length REAL;
ALTER TABLE media ADD COLUMN aperture REAL;
ALTER TABLE media ADD COLUMN exposure_time REAL;
ALTER TABLE media ADD COLUMN iso INTEGER;
ALTER TABLE media ADD COLUMN flash_fired INTEGER CHECK (flash_fired IN (0, 1));
ALTER TABLE media ADD COLUMN gps_latitude REAL;
ALTER TABLE media ADD COLUMN gps_longitude REAL;
ALTER TABLE media ADD COLUMN gps_altitude REAL;
ALTER TABLE media ADD COLUMN duration_ms INTEGER;

UPDATE media
SET original_file_name = file_name,
    name_key = lower(file_name),
    capture_time = strftime('%Y-%m-%dT%H:%M:%S', file_modified_at / 1000, 'unixepoch', 'localtime'),
    capture_time_source = 'file',
    format = CASE
        WHEN lower(file_name) LIKE '%.jpg' OR lower(file_name) LIKE '%.jpeg' THEN 'jpeg'
        WHEN lower(file_name) LIKE '%.png' THEN 'png'
        WHEN lower(file_name) LIKE '%.heic' OR lower(file_name) LIKE '%.heif' THEN 'heic'
        WHEN lower(file_name) LIKE '%.tif' OR lower(file_name) LIKE '%.tiff' THEN 'tiff'
        WHEN lower(file_name) LIKE '%.cr2' THEN 'cr2'
        WHEN lower(file_name) LIKE '%.cr3' THEN 'cr3'
        WHEN lower(file_name) LIKE '%.nef' THEN 'nef'
        WHEN lower(file_name) LIKE '%.arw' THEN 'arw'
        WHEN lower(file_name) LIKE '%.raf' THEN 'raf'
        WHEN lower(file_name) LIKE '%.dng' THEN 'dng'
        WHEN lower(file_name) LIKE '%.mp4' THEN 'mp4'
        WHEN lower(file_name) LIKE '%.mov' THEN 'mov'
        WHEN lower(file_name) LIKE '%.m4v' THEN 'm4v'
        WHEN lower(file_name) LIKE '%.avi' THEN 'avi'
        WHEN lower(file_name) LIKE '%.mts' OR lower(file_name) LIKE '%.m2ts' THEN 'mts'
        WHEN lower(file_name) LIKE '%.3gp' THEN '3gp'
        WHEN lower(file_name) LIKE '%.mkv' THEN 'mkv'
        WHEN kind = 'video' THEN 'mov'
        ELSE 'jpeg'
    END;

-- Recherche des doublons : même nom d'origine, même taille, même date.
CREATE INDEX media_duplicates ON media (name_key, file_size, capture_time);
-- Tri par date de prise de vue.
CREATE INDEX media_by_capture_time ON media (capture_time);

-- Imports -------------------------------------------------------------------

ALTER TABLE imports ADD COLUMN status TEXT NOT NULL DEFAULT 'completed'
    CHECK (status IN ('running', 'completed', 'cancelled', 'failed'));
ALTER TABLE imports ADD COLUMN added_photos INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN added_videos INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN already_in_catalog INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN duplicates_skipped INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN without_exif_date INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN unreadable_metadata INTEGER NOT NULL DEFAULT 0;

UPDATE imports SET status = 'running' WHERE finished_at IS NULL;
