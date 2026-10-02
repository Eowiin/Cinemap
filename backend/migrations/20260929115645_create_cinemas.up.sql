-- Add migration script here

CREATE TABLE cinemas (
    id              TEXT PRIMARY KEY,     -- ID AlloCiné
    name            TEXT NOT NULL,
    name_search     TEXT NOT NULL,        -- nom normalisé (minuscules, sans accents) pour la recherche
    address         TEXT,
    postal_code     TEXT,
    city            TEXT,
    city_search     TEXT,                 -- ville normalisée, pour la recherche
    insee_code      TEXT,
    department      TEXT,
    lat             REAL,
    lng             REAL,
    geocode_score   REAL,
    cnc_id          INTEGER,
    screens         INTEGER,
    seats           INTEGER,
    art_et_essai    INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL
);
