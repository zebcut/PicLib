-- Migration 32 : rangement des copies, un choix de
-- l'écran d'import retenu pour le suivant, écrit en JSON
-- (`DestinationLayout`) ; NULL : `AAAA/MM/JJ`, le rangement d'avant.
ALTER TABLE import_preferences ADD COLUMN layout TEXT;
