-- Où voir un film qui n'est plus en salle (TMDB, données JustWatch, France).
-- JSON {link, flatrate, rent, buy} ; NULL = jamais lu. Date de lecture : tmdb_synced_at.
ALTER TABLE movies ADD COLUMN watch_providers TEXT;
-- Dernier jour ciné passé où le film avait une séance, noté avant la purge des séances.
ALTER TABLE movies ADD COLUMN last_showtime_date TEXT;
