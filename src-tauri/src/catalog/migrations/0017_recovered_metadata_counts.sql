-- Migration 17 : compteurs de la récupération des métadonnées à l'import
-- : fichiers avec des mots-clés récupérés, fichiers
-- dont les sources de métadonnées se contredisaient.
ALTER TABLE imports ADD COLUMN with_recovered_keywords INTEGER NOT NULL DEFAULT 0;
ALTER TABLE imports ADD COLUMN metadata_conflicts INTEGER NOT NULL DEFAULT 0;
