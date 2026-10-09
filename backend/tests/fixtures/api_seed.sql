-- Base de test de l'API (tests/api.rs), insérée après les migrations.
-- Dates en 2099 : toujours « à venir », pour que les filtres `date >= aujourd'hui`
-- (dates, recherche, meta) ne dépendent pas du jour où les tests tournent.
-- Centre de référence : Paris (48.8566, 2.3522) ; 0.009° de latitude ≈ 1 km.

INSERT INTO cinemas (id, name, name_search, city, city_search, address, postal_code, department, lat, lng, art_et_essai, updated_at) VALUES
    ('PARIS1', 'UGC Ciné Cité Les Halles', 'ugc cine cite les halles', 'Paris', 'paris', '7 Place de la Rotonde', '75001', '75', 48.8656, 2.3522, 1, datetime('now')),
    ('PARIS2', 'Le Méliès', 'le melies', 'Montreuil', 'montreuil', NULL, '93100', '93', 48.9464, 2.3522, 0, datetime('now')),
    ('LYON', 'Pathé Bellecour', 'pathe bellecour', 'Lyon', 'lyon', NULL, '69002', '69', 45.7640, 4.8357, 0, datetime('now')),
    ('NOGEO', 'Cinéma sans GPS', 'cinema sans gps', 'Paris', 'paris', NULL, NULL, '75', NULL, NULL, 0, datetime('now')),
    ('OLD', 'Cinéma fermé', 'cinema ferme', 'Paris', 'paris', NULL, NULL, '75', 48.8570, 2.3525, 0, '2000-01-01 00:00:00');

INSERT INTO movies (id, title, title_search, genres, directors, cast_members, countries, updated_at) VALUES
    (1001, 'L''Été dernier', 'l''ete dernier', '["Drame"]', '["Catherine Breillat"]', '[{"name":"Léa Drucker","role":"Anne"}]', '["France"]', datetime('now')),
    (1002, 'Batman', 'batman', '["Action"]', '[]', '[]', '[]', datetime('now')),
    (1003, 'Sans séance', 'sans seance', '[]', '[]', '[]', '[]', datetime('now'));

INSERT INTO showtimes (id, cinema_id, movie_id, date, starts_at, version, formats, booking_url) VALUES
    -- Film A (1001) à PARIS1 : VF, VOST, et une séance après minuit rattachée au 01/01.
    ('A1', 'PARIS1', 1001, '2099-01-01', '2099-01-01T14:00:00', 'VF', '["IMAX"]', 'https://x/A1'),
    ('A2', 'PARIS1', 1001, '2099-01-01', '2099-01-01T22:30:00', 'VOST', '[]', NULL),
    ('A3', 'PARIS1', 1001, '2099-01-01', '2099-01-02T00:15:00', 'VF', '[]', NULL),
    ('A4', 'PARIS1', 1001, '2099-01-02', '2099-01-02T20:00:00', 'VF', '[]', NULL),
    ('A5', 'PARIS2', 1001, '2099-01-01', '2099-01-01T18:00:00', 'VF', '[]', NULL),
    ('A6', 'LYON', 1001, '2099-01-01', '2099-01-01T19:00:00', 'VF', '[]', NULL),
    -- Cinémas invisibles : ces séances ne doivent apparaître nulle part.
    ('A7', 'NOGEO', 1001, '2099-01-01', '2099-01-01T10:00:00', 'VF', '[]', NULL),
    ('A8', 'OLD', 1001, '2099-01-01', '2099-01-01T10:00:00', 'VF', '[]', NULL),
    ('B1', 'NOGEO', 1002, '2099-01-01', '2099-01-01T11:00:00', 'VO', '[]', NULL),
    -- Film B (1002) : une seule séance visible, en VO.
    ('B2', 'PARIS1', 1002, '2099-01-01', '2099-01-01T16:00:00', 'VO', '[]', NULL);

INSERT INTO cards (id, name, source_url, updated_at) VALUES
    ('ugc_illimite', 'UGC Illimité', 'https://www.ugc.fr/cinemas-acceptant-ui.html', '2099-01-01 02:14:00'),
    ('pathe_cinepass', 'Pathé CinéPass', 'https://www.pathe.fr/api/cinemas', '2099-01-01 02:14:00');

INSERT INTO cinema_cards (cinema_id, card_id, manual) VALUES
    ('PARIS1', 'ugc_illimite', 0),
    ('PARIS1', 'pathe_cinepass', 1),
    ('LYON', 'pathe_cinepass', 0),
    -- Cinéma masqué : son lien ne doit apparaître nulle part.
    ('OLD', 'ugc_illimite', 0);
