-- Add migration script here

CREATE INDEX idx_showtimes_cinema_date ON showtimes(cinema_id, date);
CREATE INDEX idx_showtimes_movie_date  ON showtimes(movie_id, date);
CREATE INDEX idx_showtimes_date        ON showtimes(date);
