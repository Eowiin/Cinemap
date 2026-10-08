-- Add migration script here

CREATE TABLE showtimes (
    id          TEXT PRIMARY KEY,          -- ID séance AlloCiné
    cinema_id   TEXT    NOT NULL REFERENCES cinemas(id) ON DELETE CASCADE,
    movie_id    INTEGER NOT NULL REFERENCES movies(id),
    date        TEXT    NOT NULL,          -- jour ciné (jour demandé à AlloCiné), peut différer de la date de starts_at
    starts_at   TEXT    NOT NULL,
    version     TEXT    NOT NULL,          -- VF | VO | VOST
    formats     TEXT    NOT NULL DEFAULT '[]',  -- JSON
    booking_url TEXT
);
