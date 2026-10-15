-- Migration 27 : piles.
--
-- Une pile regroupe des photos d'un même dossier ; une photo n'appartient
-- qu'à une pile (clé primaire `media_id`). Le rang le plus petit est la
-- couverture. Une pile repliée n'affiche qu'une vignette ; une nouvelle
-- pile est repliée.
--
-- Une pile vidée par le retrait de ses photos (ou réduite à une seule) est
-- gardée pour qu'une annulation la retrouve ; l'affichage l'ignore, et elle
-- disparaît à l'ouverture suivante du catalogue.
CREATE TABLE stacks (
    id        INTEGER PRIMARY KEY,
    collapsed INTEGER NOT NULL DEFAULT 1 CHECK (collapsed IN (0, 1))
) STRICT;

CREATE TABLE stack_media (
    media_id INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    stack_id INTEGER NOT NULL REFERENCES stacks (id) ON DELETE CASCADE,
    position INTEGER NOT NULL
) STRICT;

CREATE INDEX stack_media_by_stack ON stack_media (stack_id, position);
