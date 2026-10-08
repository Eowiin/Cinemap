-- Add migration script here

CREATE TABLE scrape_runs (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,             -- cinemas | showtimes | tmdb
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    ok_count    INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0
);
