-- Migration 7 : métadonnées descriptives modifiables.
--
-- NULL : champ vide.
ALTER TABLE media ADD COLUMN title TEXT;
ALTER TABLE media ADD COLUMN caption TEXT;
ALTER TABLE media ADD COLUMN creator TEXT;
ALTER TABLE media ADD COLUMN copyright TEXT;
ALTER TABLE media ADD COLUMN country TEXT;
ALTER TABLE media ADD COLUMN region TEXT;
ALTER TABLE media ADD COLUMN city TEXT;
ALTER TABLE media ADD COLUMN location TEXT;
