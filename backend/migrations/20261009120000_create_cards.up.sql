-- Cartes d'abonnement (UGC Illimité, Pathé CinéPass…) et cinémas qui les acceptent.
-- Les lignes de `cards` sont écrites par `import-cards`, pas par une migration :
-- ajouter une carte ne demande pas de changer le schéma.
CREATE TABLE cards (
    id          TEXT PRIMARY KEY,   -- 'ugc_illimite', 'pathe_cinepass'
    name        TEXT NOT NULL,      -- 'UGC Illimité'
    source_url  TEXT,
    updated_at  TEXT NOT NULL       -- dernier import réussi de la liste
);

-- Table de liaison pure : WITHOUT ROWID évite un rowid et un index caché en plus de la PK.
CREATE TABLE cinema_cards (
    cinema_id   TEXT NOT NULL REFERENCES cinemas(id) ON DELETE CASCADE,
    card_id     TEXT NOT NULL REFERENCES cards(id),
    manual      INTEGER NOT NULL DEFAULT 0,  -- 1 : ligne de docs/data/cartes.csv
    PRIMARY KEY (cinema_id, card_id)
) WITHOUT ROWID;

-- La PK sert « les cartes d'un cinéma » ; celui-ci sert « les cinémas d'une carte ».
CREATE INDEX idx_cinema_cards_card ON cinema_cards(card_id);
