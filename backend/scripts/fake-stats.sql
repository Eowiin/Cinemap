-- Fausses données de mesure d'audience (30 jours), pour essayer `cargo run -- stats` en local.
-- Usage, depuis backend/ : sqlite3 database.db < scripts/fake-stats.sql
-- Remplace tous les compteurs existants : jamais sur la base de prod.

DELETE FROM daily_events;

-- Un trafic qui grossit (environ ×3 en un mois), plus fort le week-end.
CREATE TEMP TABLE fake_days AS
WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 29)
SELECT date('now', 'localtime', '-' || i || ' days') AS day,
       (30 - i) / 10.0 + 1
         + CASE WHEN strftime('%w', 'now', 'localtime', '-' || i || ' days') IN ('0', '6')
                THEN 0.8 ELSE 0 END AS boost
FROM n;

-- Les cinémas et les films qui ont le plus de séances, classés : les premiers sont les plus vus.
CREATE TEMP TABLE fake_cinemas AS
SELECT cinema_id AS id, row_number() OVER (ORDER BY count(*) DESC) AS rank
FROM showtimes GROUP BY cinema_id ORDER BY count(*) DESC LIMIT 40;

CREATE TEMP TABLE fake_movies AS
SELECT movie_id AS id, row_number() OVER (ORDER BY count(*) DESC) AS rank
FROM showtimes GROUP BY movie_id ORDER BY count(*) DESC LIMIT 50;

-- Événements sans cible : moyenne par jour avant boost.
INSERT INTO daily_events (day, event, target, count)
SELECT day, event, '', count FROM (
    SELECT d.day, e.event, CAST(e.mean * d.boost * (0.7 + abs(random() % 60) / 100.0) AS INTEGER) AS count
    FROM fake_days d
    CROSS JOIN (SELECT 'view_home' AS event, 25 AS mean
                UNION ALL SELECT 'view_programme', 6
                UNION ALL SELECT 'compose', 1.2
                UNION ALL SELECT 'share', 0.8
                UNION ALL SELECT 'calendar', 0.6
                UNION ALL SELECT 'keep_shared', 0.4) e
) WHERE count > 0;

-- Par cinéma : pages, réservations, ajouts, décroissants avec le rang.
INSERT INTO daily_events (day, event, target, count)
SELECT day, event, id, count FROM (
    SELECT d.day, e.event, c.id,
           CAST(e.mean * d.boost / (1 + c.rank / 5.0) * (abs(random() % 100) / 100.0) AS INTEGER) AS count
    FROM fake_days d
    CROSS JOIN fake_cinemas c
    CROSS JOIN (SELECT 'view_cinema' AS event, 6 AS mean
                UNION ALL SELECT 'book', 3
                UNION ALL SELECT 'add', 2.5) e
) WHERE count > 0;

INSERT INTO daily_events (day, event, target, count)
SELECT day, 'view_movie', CAST(id AS TEXT), count FROM (
    SELECT d.day, m.id,
           CAST(10 * d.boost / (1 + m.rank / 4.0) * (abs(random() % 100) / 100.0) AS INTEGER) AS count
    FROM fake_days d CROSS JOIN fake_movies m
) WHERE count > 0;

INSERT INTO daily_events (day, event, target, count)
SELECT day, 'view_city', slug, count FROM (
    SELECT d.day, v.slug,
           CAST(v.mean * d.boost * (abs(random() % 100) / 100.0) AS INTEGER) AS count
    FROM fake_days d
    CROSS JOIN (SELECT 'paris' AS slug, 8 AS mean
                UNION ALL SELECT 'lyon', 3
                UNION ALL SELECT 'marseille', 2.5
                UNION ALL SELECT 'montreuil', 2
                UNION ALL SELECT 'lille', 1.5
                UNION ALL SELECT 'nantes', 1.5
                UNION ALL SELECT 'bordeaux', 1.2) v
) WHERE count > 0;

SELECT count(*) || ' lignes, ' || sum(count) || ' événements sur ' || count(DISTINCT day) || ' jours'
FROM daily_events;
