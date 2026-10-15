-- Migration 9 : la position GPS 0, 0 devient « pas de position ».
--
-- C'est ce qu'enregistre un appareil qui n'a pas trouvé sa position ; les
-- imports faits avant cette version l'ont gardée comme une vraie position
-- (en mer, dans le golfe de Guinée). La lecture des métadonnées l'écarte
-- désormais ; cette migration corrige les photos déjà au catalogue.

-- Leur trace de recherche de lieu (aucun lieu n'avait été trouvé) est
-- retirée : si une vraie position leur est donnée plus tard, leur lieu
-- pourra être cherché.
DELETE FROM place_lookups
WHERE media_id IN (
    SELECT id FROM media WHERE gps_latitude = 0 AND gps_longitude = 0
);

UPDATE media
SET gps_latitude = NULL, gps_longitude = NULL, gps_altitude = NULL
WHERE gps_latitude = 0 AND gps_longitude = 0;
