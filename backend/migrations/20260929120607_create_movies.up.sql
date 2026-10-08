-- Add migration script here

CREATE TABLE movies (
    id              INTEGER PRIMARY KEY,  -- ID AlloCiné
    title           TEXT NOT NULL,
    title_search    TEXT NOT NULL,
    original_title  TEXT,
    poster_url      TEXT,
    synopsis        TEXT,
    runtime_min     INTEGER,
    release_date    TEXT,
    production_year INTEGER,
    certificate     TEXT,
    genres          TEXT NOT NULL DEFAULT '[]',   -- JSON
    directors       TEXT NOT NULL DEFAULT '[]',   -- JSON
    cast_members    TEXT NOT NULL DEFAULT '[]',   -- JSON [{name, role}]
    countries       TEXT NOT NULL DEFAULT '[]',   -- JSON
    tmdb_id         INTEGER,
    backdrop_url    TEXT,
    trailer_url     TEXT,
    rating          REAL,                 -- TMDB, /10
    user_rating     REAL,                 -- AlloCiné spectateurs, /5
    tmdb_synced_at  TEXT,
    updated_at      TEXT NOT NULL
);
