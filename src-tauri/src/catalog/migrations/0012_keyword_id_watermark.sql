-- Migration 12 : un mot-clé créé n'hérite jamais du numéro d'un mot-clé
-- supprimé.
--
-- Sans cela, SQLite réattribue le plus grand identifiant d'une ligne
-- supprimée : annuler la suppression d'un mot-clé échouerait (son numéro
-- serait pris), et ce qui le désigne par son numéro (les cases des jeux de
-- mots-clés) désignerait un autre mot-clé. Même mécanisme que pour les
-- médias (migration 6).
INSERT INTO id_watermarks (name, last_id)
    VALUES ('keywords', (SELECT coalesce(max(id), 0) FROM keywords));
