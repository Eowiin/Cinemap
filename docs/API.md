# Contrat d'API — Cinemap

Contrat entre le backend (Rust) et le frontend (Svelte). **Toute modification se fait d'abord ici.**

## Conventions

- Préfixe `/api`, `GET` uniquement, JSON en UTF-8.
- **Dates** : `YYYY-MM-DD`, jour calendaire à Paris. Par défaut = aujourd'hui à Paris (pas en UTC).
- **Horaires** : `starts_at` = `YYYY-MM-DDTHH:MM:SS`, heure locale Paris, sans fuseau (tel que fourni par AlloCiné).
- **IDs** : cinéma = ID AlloCiné (`"C0159"`, string) ; film = ID AlloCiné (`1000032855`, entier).
- **Position** : paramètres `lat` et `lng` (floats). `distance_km` n'est présent que si `lat`/`lng` sont fournis.
- Champs inconnus = `null` (jamais absents). Listes vides = `[]`.
- **Erreurs** : code HTTP approprié + corps
  ```json
  { "error": { "code": "not_found", "message": "Cinéma introuvable" } }
  ```
  Codes : `bad_request` (400), `not_found` (404), `internal` (500).
- `version` d'une séance : `"VF"` | `"VO"` | `"VOST"`.

## Types partagés

```ts
type CinemaSummary = {
  id: string;              // "C0159"
  name: string;            // "UGC Ciné Cité Les Halles"
  city: string | null;
  lat: number;
  lng: number;
  art_et_essai: boolean;
  distance_km?: number;    // si lat/lng fournis
};

type Cinema = CinemaSummary & {
  address: string | null;      // "7 Place de la Rotonde"
  postal_code: string | null;  // "75001"
  department: string | null;   // "Paris"
  screens: number | null;
  seats: number | null;
  allocine_url: string;        // https://www.allocine.fr/seance/salle_gen_csalle=C0159.html
};

type MovieSummary = {
  id: number;
  title: string;
  poster_url: string | null;
  genres: string[];            // ["Comédie", "Drame"]
  runtime_min: number | null;
  release_date: string | null; // "2026-09-30"
};

type Person = { name: string; role: string | null };  // role = personnage pour le casting

type Movie = MovieSummary & {
  original_title: string | null;
  synopsis: string | null;     // texte brut
  directors: string[];
  cast: Person[];              // 10 premiers max
  countries: string[];
  production_year: number | null;
  certificate: string | null;  // "Interdit - 16 ans"
  // Enrichissement TMDB (null tant que non fait)
  backdrop_url: string | null;
  trailer_url: string | null;  // URL YouTube
  rating: number | null;       // /10
};

type Showtime = {
  id: string;
  starts_at: string;           // "2026-09-25T20:00:00"
  version: "VF" | "VO" | "VOST";
  formats: string[];           // ["IMAX"], ["3D"], ["4DX"], []
  booking_url: string | null;
};
```

## Endpoints

### `GET /api/meta`

État des données, pour afficher « mis à jour il y a X » et borner le sélecteur de date.

```json
{
  "last_scrape_at": "2026-09-25T04:12:00Z",
  "dates_available": ["2026-09-25", "2026-09-26", "2026-09-27"],
  "cinema_count": 2051,
  "movie_count": 342
}
```

### `GET /api/cinemas`

Tous les cinémas géolocalisés, pour la carte (~2 000 éléments, gzip ≈ 60 Ko).

Query : `art_et_essai?: bool`, `lat?`, `lng?`.

Réponse : `CinemaSummary[]`.

### `GET /api/cinemas/{id}`

Réponse : `Cinema`. 404 si inconnu.

### `GET /api/cinemas/{id}/showtimes`

Query : `date?`, `version?` (`VO` = VO + VOST), `after?` (`HH:MM`, séances qui commencent à partir de cette heure).

```json
{
  "cinema": Cinema,
  "date": "2026-09-25",
  "movies": [
    { "movie": MovieSummary, "showtimes": [Showtime, …] }
  ]
}
```

Films triés par titre, séances par heure.

### `GET /api/movies`

Les films « à l'affiche » un jour donné (page d'accueil).

Query : `date?`, `lat?`, `lng?`, `radius_km?` (défaut 15 si lat/lng fournis), `version?`, `after?`, `limit?` (défaut 50).

```json
{
  "date": "2026-09-25",
  "movies": [
    {
      "movie": MovieSummary,
      "cinema_count": 42,          // dans le rayon si position fournie
      "showtime_count": 187,
      "next_showtime": "2026-09-25T20:00:00"  // prochaine séance (≥ after) dans la zone
    }
  ]
}
```

Tri par `cinema_count` décroissant.

### `GET /api/movies/{id}`

Réponse : `Movie`. 404 si inconnu.

### `GET /api/movies/{id}/showtimes`

Où et quand voir un film.

Query : `date?`, `lat?`, `lng?`, `radius_km?` (pas de rayon par défaut), `version?`, `after?`.

```json
{
  "movie": MovieSummary,
  "date": "2026-09-25",
  "cinemas": [
    { "cinema": CinemaSummary, "showtimes": [Showtime, …] }
  ]
}
```

Tri : par `distance_km` si position fournie, sinon par nom de cinéma.

### `GET /api/search`

Query : `q` (≥ 2 caractères, sinon 400).

Recherche insensible à la casse **et aux accents** (`"cine cite"` trouve `"Ciné Cité"`), sur les films ayant au moins une séance dans les jours disponibles, et sur les cinémas.

```json
{
  "movies":  [MovieSummary, …],   // 8 max, les plus diffusés d'abord
  "cinemas": [CinemaSummary, …]   // 8 max
}
```

## Schéma SQLite (proposition)

```sql
CREATE TABLE cinemas (
    id              TEXT PRIMARY KEY,     -- ID AlloCiné
    name            TEXT NOT NULL,
    name_search     TEXT NOT NULL,        -- nom normalisé (minuscules, sans accents) pour la recherche
    address         TEXT,
    postal_code     TEXT,
    city            TEXT,
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
    rating          REAL,
    tmdb_synced_at  TEXT,
    updated_at      TEXT NOT NULL
);

CREATE TABLE showtimes (
    id          TEXT PRIMARY KEY,          -- ID séance AlloCiné
    cinema_id   TEXT    NOT NULL REFERENCES cinemas(id) ON DELETE CASCADE,
    movie_id    INTEGER NOT NULL REFERENCES movies(id),
    date        TEXT    NOT NULL,
    starts_at   TEXT    NOT NULL,
    version     TEXT    NOT NULL,          -- VF | VO | VOST
    formats     TEXT    NOT NULL DEFAULT '[]',  -- JSON
    booking_url TEXT
);
CREATE INDEX idx_showtimes_cinema_date ON showtimes(cinema_id, date);
CREATE INDEX idx_showtimes_movie_date  ON showtimes(movie_id, date);
CREATE INDEX idx_showtimes_date        ON showtimes(date);

CREATE TABLE scrape_runs (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,             -- cinemas | showtimes | tmdb
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    ok_count    INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0
);
```

Au démarrage : `PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;`.

Distance : SQLite n'a pas de fonctions géo → filtre grossier par bounding box en SQL, puis calcul haversine et tri en Rust.
