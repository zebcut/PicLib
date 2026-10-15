-- Migration 14 : date de dernière utilisation de chaque mot-clé, pour
-- proposer les mots-clés récents(suggestions).
--
-- Millisecondes depuis 1970 (UTC) ; NULL pour un mot-clé jamais posé depuis
-- cette migration.
ALTER TABLE keywords ADD COLUMN last_used_at INTEGER;

CREATE INDEX keywords_by_last_used ON keywords (last_used_at)
    WHERE last_used_at IS NOT NULL;
