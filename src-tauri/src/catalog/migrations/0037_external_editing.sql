-- Migration 37 : retouche dans un éditeur externe.
--
-- Photos envoyées à un éditeur. Le dossier d'une photo suivie est surveillé
-- : un fichier qui y apparaît après l'envoi, au nom dérivé
-- du sien, devient une version traitée. `sent_at` : premier envoi (les
-- fichiers plus anciens ne sont pas des exports de l'éditeur) ;
-- `active_at` : dernier envoi ou dernier export reçu ; le suivi s'arrête
-- 30 jours après. Millisecondes.
CREATE TABLE editor_sessions (
    media_id  INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    editor    TEXT    NOT NULL,
    sent_at   INTEGER NOT NULL,
    active_at INTEGER NOT NULL,
    PRIMARY KEY (media_id, editor)
) STRICT, WITHOUT ROWID;

-- Lien « version traitée de » : une ligne par version. La version reste au
-- catalogue si l'original le quitte (le lien disparaît avec lui).
--
-- `editor` : éditeur qui l'a produite (NULL : inconnu). `working_id` : la
-- photo envoyée à l'éditeur (le RAW, ou la copie `-Modifié`), que
-- « Modifier de nouveau » lui renvoie, sauf s'il a
-- enregistré son travail à calques à côté (PSD, `.afphoto`) ; NULL :
-- l'original.
-- `copy_digest` : empreinte SHA-256 d'une copie `-Modifié` à sa création,
-- pour reconnaître une copie que l'éditeur n'a pas modifiée.
CREATE TABLE edited_versions (
    media_id     INTEGER PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    original_id  INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    editor       TEXT,
    working_id   INTEGER REFERENCES media (id) ON DELETE SET NULL,
    copy_digest  TEXT,
    CHECK (media_id <> original_id)
) STRICT;

CREATE INDEX edited_versions_by_original ON edited_versions (original_id);

-- Les imports qui n'ajoutent que des versions traitées ne sont pas des
-- imports de l'utilisateur : ils ne deviennent pas « Import précédent ».
ALTER TABLE imports ADD COLUMN edited_versions INTEGER NOT NULL DEFAULT 0
    CHECK (edited_versions IN (0, 1));
