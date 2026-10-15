-- Migration 13 : jeux de mots-clés.
--
-- Un jeu a un nom et neuf cases (Option+1 à Option+9). Un seul jeu est
-- actif à la fois : celui du panneau « Mots-clés » et des raccourcis.
CREATE TABLE keyword_sets (
    id       INTEGER PRIMARY KEY,
    name     TEXT    NOT NULL CHECK (name <> ''),
    -- Forme composée en minuscules : « Voyage » et « voyage » sont le même
    -- jeu.
    name_key TEXT    NOT NULL UNIQUE,
    active   INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1))
) STRICT;

CREATE UNIQUE INDEX keyword_sets_single_active ON keyword_sets (active) WHERE active = 1;

-- Cases remplies d'un jeu. Pas de clé étrangère vers `keywords` : une case
-- dont le mot-clé est supprimé paraît vide, et se remplit de nouveau si la
-- suppression est annulée (le mot-clé revient avec son numéro, jamais
-- redonné à un autre : migration 12).
CREATE TABLE keyword_set_slots (
    set_id     INTEGER NOT NULL REFERENCES keyword_sets (id) ON DELETE CASCADE,
    slot       INTEGER NOT NULL CHECK (slot BETWEEN 1 AND 9),
    keyword_id INTEGER NOT NULL,
    PRIMARY KEY (set_id, slot)
) STRICT, WITHOUT ROWID;

CREATE INDEX keyword_set_slots_by_keyword ON keyword_set_slots (keyword_id);
