-- Mesure d'audience sans cookie ni identifiant : un compteur par jour, événement et cible.
-- Aucune ligne ne désigne un visiteur (ni IP, ni navigateur, ni heure précise).
CREATE TABLE daily_events (
    day     TEXT NOT NULL,              -- jour à Paris, YYYY-MM-DD
    event   TEXT NOT NULL,              -- 'view_movie', 'book'… (voir src/stats.rs)
    target  TEXT NOT NULL DEFAULT '',   -- id du cinéma ou du film, slug de la ville ; '' sinon
    count   INTEGER NOT NULL,
    PRIMARY KEY (day, event, target)
) WITHOUT ROWID;
