-- Migration 15 : modèles de métadonnées.
--
-- Un modèle a un nom unique. Pour chaque champ, trois choix : ne rien
-- changer (pas de ligne), remplir (`value` renseigné) ou vider (`value`
-- NULL). Ses mots-clés sont ajoutés aux photos.
CREATE TABLE metadata_templates (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL CHECK (name <> ''),
    -- Forme composée en minuscules : « Famille » et « famille » sont le
    -- même modèle.
    name_key TEXT    NOT NULL UNIQUE
) STRICT;

CREATE TABLE metadata_template_fields (
    template_id INTEGER NOT NULL REFERENCES metadata_templates (id) ON DELETE CASCADE,
    field       TEXT    NOT NULL CHECK (field IN ('title', 'caption', 'creator', 'copyright',
                                                  'country', 'region', 'city', 'location')),
    value       TEXT    CHECK (value IS NULL OR value <> ''),
    PRIMARY KEY (template_id, field)
) STRICT, WITHOUT ROWID;

-- Pas de clé étrangère vers `keywords`, comme pour les jeux (migration 13) :
-- un mot-clé supprimé est ignoré, et revient si la suppression est annulée.
CREATE TABLE metadata_template_keywords (
    template_id INTEGER NOT NULL REFERENCES metadata_templates (id) ON DELETE CASCADE,
    keyword_id  INTEGER NOT NULL,
    PRIMARY KEY (template_id, keyword_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX metadata_template_keywords_by_keyword ON metadata_template_keywords (keyword_id);
