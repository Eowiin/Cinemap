import sqlite3
from pathlib import Path

DB_PATH = Path(__file__).parent.parent.parent / "data" / "cinemas.db"


def get_connection() -> sqlite3.Connection:
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn


def init_db():
    with get_connection() as conn:
        conn.executescript("""
            CREATE TABLE IF NOT EXISTS cinemas (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id       TEXT    UNIQUE NOT NULL,
                name            TEXT    NOT NULL,
                address         TEXT,
                city            TEXT,
                zip_code        TEXT,
                department      TEXT,
                lat             REAL,
                lng             REAL,
                screens         INTEGER,
                seats           INTEGER,
                is_art_et_essai INTEGER DEFAULT 0,
                is_multiplex    INTEGER DEFAULT 0,
                allocine_id     TEXT,
                scraped_at      TEXT    DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS showtimes (
                id                   INTEGER PRIMARY KEY AUTOINCREMENT,
                cinema_id            INTEGER NOT NULL REFERENCES cinemas(id),
                allocine_showtime_id TEXT,
                movie_title          TEXT    NOT NULL,
                movie_allocine_id    TEXT,
                poster_url           TEXT,
                starts_at            TEXT    NOT NULL,
                date                 TEXT    NOT NULL,
                version              TEXT,
                experience           TEXT,
                scraped_at           TEXT    DEFAULT (datetime('now')),
                UNIQUE(cinema_id, allocine_showtime_id)
            );

            CREATE INDEX IF NOT EXISTS idx_showtimes_cinema_date
                ON showtimes(cinema_id, date);
            CREATE INDEX IF NOT EXISTS idx_showtimes_date
                ON showtimes(date);
            CREATE INDEX IF NOT EXISTS idx_cinemas_allocine
                ON cinemas(allocine_id);
        """)
