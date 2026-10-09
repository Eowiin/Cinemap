# Contrat d'API — Cinemap

Contrat entre le backend (Rust) et le frontend (Svelte). **Toute modification se fait d'abord ici.**

## Conventions

- Préfixe `/api`, `GET` uniquement, JSON en UTF-8.
- **Dates** : `YYYY-MM-DD`, **jour ciné** à Paris. Par défaut = aujourd'hui à Paris (pas en UTC).
  - Le jour ciné est le jour demandé à AlloCiné (`d-{date}`) : une séance à 00h15 renvoyée pour le 25 appartient au 25 (son `starts_at` est le 26 à 00:15). Elle apparaît en fin de programme du 25, jamais dans celui du 26.
  - Une `date` hors de `dates_available` n'est pas une erreur : réponse 200 avec des listes vides.
- **Horaires** : `starts_at` = `YYYY-MM-DDTHH:MM:SS`, heure locale Paris, sans fuseau (tel que fourni par AlloCiné).
- **`after`** (`HH:MM`) : garde les séances dont `starts_at >= "{date}T{after}:00"`. On compare le `starts_at` **complet**, pour que les séances après minuit restent incluses (`after=22:00` garde celle de 00h15).
- **IDs** : cinéma = ID AlloCiné (`"C0159"`, string) ; film = ID AlloCiné (`1000032855`, entier).
- **Position** : paramètres `lat` et `lng` (floats). `distance_km` vaut `null` si `lat`/`lng` ne sont pas fournis. Les distances sont arrondies à 0,1 km.
- Champs inconnus = `null` (jamais absents). Listes vides = `[]`.
- **Booléens en query** : `true` / `false`. Un paramètre mal formé (date, heure, booléen, nombre, `version` inconnue) → 400 `bad_request`.
  - `version` : exactement `VF` ou `VO` (majuscules ; `vf`, `VOST` → 400).
  - `after` : `HH:MM` (le zéro initial est facultatif : `9:05` est accepté).
  - `limit` : de 1 à 200 ; `radius_km` : strictement positif, 100 au plus.
  - ID de film non numérique dans le chemin (`/api/movies/abc`) → 400 `bad_request` au format JSON ci-dessous, comme toute autre erreur.
- **Cinémas non géocodés** (`lat`/`lng` NULL en base) : exclus de **tous** les endpoints, `/api/cinemas/{id}` compris (404).
- **Cinémas plus listés par AlloCiné** : un cinéma dont `updated_at` (dernière fois qu'`import-cinemas` l'a vu) date de **plus de 14 jours** est exclu de **tous** les endpoints, comme un cinéma non géocodé (404 sur `/api/cinemas/{id}`, ses séances n'apparaissent plus). Il réapparaît dès qu'un import le revoit. Voir « Cycle de vie des cinémas » plus bas.
- **Cache** : réponses **2xx** avec `Cache-Control: public, max-age=300` (utile au service worker de la PWA). Les erreurs portent `Cache-Control: no-store` : une panne ne doit pas rester en cache chez le visiteur après le retour du serveur.
- **CORS** : aucun. En prod, nginx sert le front et `/api` sur le même domaine ; en dev, le serveur Vite relaie `/api` vers `localhost:3000` (`server.proxy`).
- **Erreurs** : code HTTP approprié + corps
  ```json
  { "error": { "code": "not_found", "message": "Cinéma introuvable" } }
  ```
  Codes : `bad_request` (400), `not_found` (404), `internal` (500).
- `version` d'une séance : `"VF"` | `"VO"` | `"VOST"`.
  - `VF` = diffusé en français, **y compris un film français en version originale**. Le mapping s'appuie sur la langue du film (`movie.languages` d'AlloCiné), à vérifier empiriquement à l'étape 2.
  - `VOST` = version originale non française, **sous-titres français confirmés** par AlloCiné (tag `Localization.Subtitle.French` ou `Showtime.Accessibility.Subtitled`). `VO` = version originale non française **dont AlloCiné ne précise pas le sous-titrage** (décidé le 2026-10-08). En pratique, la plupart des `VO` en France sont sous-titrées ; le front affiche « VO » sans promettre l'absence de sous-titres.
  - Paramètre de query `version` : `VF` ou `VO`. `version=VO` renvoie `VO` + `VOST` (« pas doublé en français »).
- `formats` d'une séance : sous-ensemble de `"3D"`, `"IMAX"`, `"4DX"`, `"ScreenX"`, `"Dolby Cinema"`, `"Dolby Atmos"`. Toute autre valeur AlloCiné est ignorée (et loggée par le scraper).

## Types partagés

```ts
type CinemaSummary = {
  id: string;              // "C0159"
  name: string;            // "UGC Ciné Cité Les Halles"
  city: string | null;
  lat: number;
  lng: number;
  art_et_essai: boolean;
  cards: string[];             // ids des cartes d'abonnement acceptées (voir Card), [] si aucune
  distance_km: number | null;  // null si lat/lng non fournis
};

type Card = {
  id: string;                  // "ugc_illimite", "pathe_cinepass"
  name: string;                // "UGC Illimité"
  updated_at: string;          // "2026-10-09T02:14:00Z", dernier import réussi de la liste
};

type Cinema = CinemaSummary & {
  address: string | null;      // "7 Place de la Rotonde"
  postal_code: string | null;  // "75001"
  department: string | null;   // nom du département ("Paris"), déduit du code INSEE stocké en base via docs/data/departements.csv
  screens: number | null;
  seats: number | null;
  allocine_url: string;        // https://www.allocine.fr/seance/salle_gen_csalle=C0159.html
};

type MovieSummary = {
  id: number;
  title: string;
  poster_url: string | null;   // URL AlloCiné brute (le front choisit la taille)
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
  // AlloCiné
  user_rating: number | null;  // note spectateurs AlloCiné, /5 (stats.userRating)
};

type Showtime = {
  id: string;
  starts_at: string;           // "2026-09-25T20:00:00"
  version: "VF" | "VO" | "VOST";
  formats: string[];           // voir Conventions, ex. ["IMAX"], ["3D"], []
  booking_url: string | null;
};
```

## Endpoints

### `GET /api/meta`

État des données, pour afficher « mis à jour il y a X » et borner le sélecteur de date. Sert aussi de health check au déploiement.

```json
{
  "today": "2026-09-25",
  "last_scrape_at": "2026-09-25T04:12:00Z",
  "dates_available": ["2026-09-25", "2026-09-26", "2026-09-27"],
  "cinema_count": 2051,
  "movie_count": 342,
  "cards": [{ "id": "ugc_illimite", "name": "UGC Illimité", "updated_at": "2026-10-09T02:14:00Z" }]
}
```

`cards` : toutes les cartes connues, triées par nom (écran « mes cartes » du front).

`last_scrape_at` = `finished_at` du dernier run `kind = 'showtimes'` terminé (`finished_at IS NOT NULL`). Les runs `showtimes_partial` (`--cinema`, `--department`) sont ignorés. Les compteurs `ok_count` / `error_count` d'un run de séances comptent des couples (cinéma, date).

### `GET /api/cinemas`

Tous les cinémas visibles, pour la carte (~3 100 éléments, gzip ≈ 90 Ko). Tri par `distance_km` si `lat`/`lng` sont fournis, sinon par nom.

Query : `art_et_essai?: bool`, `lat?`, `lng?`, `cards?`.

**`cards`** (aussi sur `/api/movies` et `/api/movies/{id}/showtimes`) : ids de cartes séparés par des virgules (`cards=ugc_illimite,pathe_cinepass`). Ne garde que les cinémas qui acceptent **au moins une** de ces cartes. Id inconnu → 400 `bad_request`. Le badge est au niveau du cinéma : les restrictions par séance (avant-premières, suppléments 3D / IMAX) sont ignorées.

Réponse : `CinemaSummary[]`.

### `GET /api/cinemas/{id}`

Réponse : `Cinema`. 404 si inconnu.

### `GET /api/cinemas/{id}/showtimes`

Query : `date?`, `version?` (`VO` = VO + VOST), `after?` (`HH:MM`, séances qui commencent à partir de cette heure).

```json
{
  "cinema": Cinema,
  "date": "2026-09-25",
  "dates": ["2026-09-25", "2026-09-27"],   // jours ayant au moins une séance dans ce cinéma (filtres version/after ignorés)
  "movies": [
    { "movie": MovieSummary, "showtimes": [Showtime, …] }
  ]
}
```

Films triés par titre, séances par heure.

### `GET /api/movies`

Les films « à l'affiche » un jour donné (page d'accueil).

Query : `date?`, `lat?`, `lng?`, `radius_km?` (défaut 15 si lat/lng fournis), `version?`, `after?`, `limit?` (défaut 50, max 200), `cards?` (les compteurs ne portent alors que sur ces cinémas), `cinemas?`.

**`cinemas`** (aussi sur `/api/movies/{id}/showtimes`) : ids de cinémas séparés par des virgules (50 max), pour le filtre « mes cinémas favoris » (les favoris vivent dans le navigateur). Seuls ces cinémas comptent, **où qu'ils soient** : `radius_km` est alors ignoré (la position sert encore à `distance_km` et au tri). Id mal formé ou liste vide → 400 ; id inconnu ou masqué → simplement absent du résultat.

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

Query : `date?`, `lat`, `lng` (**obligatoires**, 400 sinon : le front a toujours une position, Paris par défaut), `radius_km?` (défaut 15, max 100), `version?`, `after?`, `cards?`, `cinemas?` (voir `/api/movies`).

```json
{
  "movie": MovieSummary,
  "date": "2026-09-25",
  "dates": ["2026-09-25", "2026-09-26"],   // jours ayant au moins une séance de ce film dans le rayon (filtres version/after ignorés)
  "cinemas": [
    { "cinema": CinemaSummary, "showtimes": [Showtime, …] }
  ]
}
```

Tri par `distance_km`.

### `GET /api/search`

Query : `q` (≥ 2 caractères, sinon 400).

Recherche insensible à la casse **et aux accents** (`"cine cite"` trouve `"Ciné Cité"`), sur les films ayant au moins une séance dans les jours disponibles, et sur les cinémas (nom **et** ville : `"montreuil"` trouve le Méliès).

Limite connue : les films sont cherchés sur leur titre français seulement (`original_title` n'a pas de colonne normalisée).

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
    city_search     TEXT,                 -- ville normalisée, pour la recherche
    insee_code      TEXT,
    department      TEXT,
    lat             REAL,
    lng             REAL,
    geocode_score   REAL,
    geocode_type    TEXT,                 -- housenumber | street | locality | municipality
    cnc_id          INTEGER,
    screens         INTEGER,
    seats           INTEGER,
    art_et_essai    INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL         -- dernière fois vu par import-cinemas (UTC, datetime('now'))
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
    rating          REAL,                 -- TMDB, /10
    user_rating     REAL,                 -- AlloCiné spectateurs, /5
    tmdb_synced_at  TEXT,
    updated_at      TEXT NOT NULL
);

CREATE TABLE showtimes (
    id          TEXT PRIMARY KEY,          -- ID séance AlloCiné
    cinema_id   TEXT    NOT NULL REFERENCES cinemas(id) ON DELETE CASCADE,
    movie_id    INTEGER NOT NULL REFERENCES movies(id),
    date        TEXT    NOT NULL,          -- jour ciné (jour demandé à AlloCiné), peut différer de la date de starts_at
    starts_at   TEXT    NOT NULL,
    version     TEXT    NOT NULL,          -- VF | VO (sous-titrage non précisé) | VOST (sous-titres confirmés)
    formats     TEXT    NOT NULL DEFAULT '[]',  -- JSON
    booking_url TEXT
);
CREATE INDEX idx_showtimes_cinema_date ON showtimes(cinema_id, date);
CREATE INDEX idx_showtimes_movie_date  ON showtimes(movie_id, date);
CREATE INDEX idx_showtimes_date        ON showtimes(date);

CREATE TABLE scrape_runs (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,             -- cinemas | showtimes | showtimes_partial | tmdb
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    ok_count    INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE cards (
    id          TEXT PRIMARY KEY,   -- 'ugc_illimite', 'pathe_cinepass'
    name        TEXT NOT NULL,      -- 'UGC Illimité'
    source_url  TEXT,
    updated_at  TEXT NOT NULL       -- dernier import automatique accepté
);

CREATE TABLE cinema_cards (
    cinema_id   TEXT NOT NULL REFERENCES cinemas(id) ON DELETE CASCADE,
    card_id     TEXT NOT NULL REFERENCES cards(id),
    manual      INTEGER NOT NULL DEFAULT 0,  -- 1 : ligne de docs/data/cartes.csv
    PRIMARY KEY (cinema_id, card_id)
) WITHOUT ROWID;
CREATE INDEX idx_cinema_cards_card ON cinema_cards(card_id);
```

Au démarrage : `PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;`.

Distance : SQLite n'a pas de fonctions géo → filtre grossier par bounding box en SQL, puis calcul haversine et tri en Rust.

### Cycle de vie des cinémas

Un cinéma n'est jamais supprimé parce qu'un seul import ne l'a pas vu : une page
AlloCiné incomplète ou un import interrompu effaceraient sinon de vrais cinémas,
avec leurs séances (`ON DELETE CASCADE`).

- `import-cinemas` met `updated_at` à jour pour chaque cinéma listé ; le géocodage et
  l'enrichissement CNC ne le modifient pas.
- **Masqué** : `updated_at` plus vieux que 14 jours → exclu de l'API (filtre dans
  La règle « visible » (géocodé + vu depuis moins de 14 jours) est définie **une seule fois**, dans la vue SQL `visible_cinemas`, utilisée par le scraper et par toutes les requêtes de l'API.
  les requêtes de `serve`, pas de colonne dédiée).
- **Supprimé** : `updated_at` plus vieux que 60 jours, à la fin d'un import
  **complet et réussi** (un import qui échoue s'arrête avant). Les séances suivent
  par cascade.
- Les durées supposent un `import-cinemas` au moins hebdomadaire.
- Le rapport de fin d'import donne le nombre de cinémas absents de cet import et
  le nombre de cinémas supprimés.

### Géocodage des cinémas

`cargo run --release -- geocode` traite les cinémas dont une coordonnée manque ;
`cargo run --release -- geocode --all` recalcule toutes les positions sans scraping.
Exécuter ces commandes depuis `backend/`. Le géocodage utilise au plus deux appels
CSV successifs à l’IGN : adresse nettoyée avec filtre postal, puis un second lot
pour les résultats refusés. Ce lot contient, par cinéma, l’adresse complète sans
filtre postal (notamment pour les CEDEX) et une recherche CP + ville limitée au
type `municipality`. Ces variantes sont regroupées dans le même appel HTTP.
Une cellule de filtre postal vide est acceptée par l’IGN, mais le programme ne
valide pas automatiquement un résultat sans code postal source.

Le code postal stocké provient de l’adresse AlloCiné, y compris après un repli.
La ville, le code INSEE et le département proviennent du résultat accepté.
Le score est informatif. `geocode_type` conserve le type réel du résultat :
`housenumber`, `street`, `locality` ou `municipality`. Une position de lieu-dit
n’est pas présentée comme le centre de la commune. Ce champ reste interne à la
base pour l’instant ; les réponses publiques ne changent pas.

Le repli exige un département compatible ET un lieu reconnaissable. Le nom
normalisé est comparé à `result_city`, `result_oldcity`, et, pour `locality`, à
une composante entière de `result_name` séparée par une virgule ou des parenthèses.
Un nom de rue ne sert jamais d’alias de commune. À code postal identique, un nom
abrégé ou un quartier est aussi accepté si l’un des noms de commune est le préfixe
complet de l’autre, sur une frontière de mots (Les Adrets / Les Adrets-de-l’Estérel,
Cannes La Bocca / Cannes). Ce contrôle reste une heuristique, pas un référentiel
exhaustif des communes. Un simple département commun ne suffit pas.

`result_context` est utilisé uniquement pour vérifier la cohérence du département
avec le code INSEE, jamais pour identifier la commune. Pour Paris, Lyon et Marseille,
le code INSEE de l’arrondissement est exigé. Décision F-ter : Riboux (13780, Var)
et Saint-Pierre-Laval (42620, Allier) restent non résolus, car leur département ne
correspond pas au préfixe postal ; aucune exception silencieuse n’est ajoutée.

Sans code postal exploitable, la position reste non validée. La forme espacée
`31 300 Toulouse` est normalisée seulement dans un suffixe CP + ville ; les
variantes ambiguës restent à examiner. L’adresse originale n’est pas réécrite :
un contrôle SQL recherchant littéralement `31300` dans celle-ci doit tenir compte
de cet espace.

Un échec de validation efface les anciennes données géographiques, sauf le code
postal extrait de l’adresse. Une erreur réseau, CSV ou SQL laisse le lot intact.
Toutes les écritures ont lieu dans une seule transaction après les appels HTTP.
Un changement d’adresse pendant les appels annule l’enregistrement du lot.

## Cartes d'abonnement

Feuille de route et décisions : [`CARTES.md`](CARTES.md). `import-cards` (aussi en fin d'`import-cinemas`) :

- **UGC Illimité** : page HTML `https://www.ugc.fr/cinemas-acceptant-ui.html`, croisée par code postal, puis même ville (codes CEDEX, codes postaux différents d'AlloCiné), puis le nom.
- **Pathé CinéPass** : réseau Pathé par l'API JSON `https://www.pathe.fr/api/cinemas` (croisement GPS : moins de 500 m, puis jusqu'à 5 km avec un seuil de nom strict) ; partenaires indépendants du PDF à la main.
- **`docs/data/cartes.csv`** (`card_id,cinema_id,source_name,commentaire`) : ajouts manuels, toujours réappliqués. `cinema_id` vide = entrée connue absente d'AlloCiné (plus signalée).
- **Garde-fou** : une liste vide, ou moins de la moitié des liens automatiques actuels, garde les anciens liens (`import-cards --force` pour accepter une liste courte).
- Les liens portent sur tous les cinémas, masqués compris : un cinéma qui réapparaît garde sa carte. L'API ne montre que les visibles.
