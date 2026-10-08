# Étape 3 : l'API (feuille de route détaillée)

Même principe que `ETAPE2.md`, en plus détaillé : pour chaque lot, **fichier à toucher → quoi y mettre → comment vérifier**. Une case = un petit pas qui compile.
Le **quoi** et le **pourquoi** sont écrits en entier. Le SQL est donné presque complet (ce n'est pas lui que tu apprends ici). Pour le Rust, tu as les signatures, les types et des indices ; le corps des fonctions, c'est toi.

Références : `API.md` (le contrat, **fait foi** : toute modification se fait d'abord là-bas), `SOBRIETE.md`, `ETAPE2.md` (pour le style des tests sur base en mémoire).

Dernière mise à jour : 2026-10-08. **Où tu en es : lots 0 à K écrits par Claude à ta demande (2026-10-08)**, `cargo test` (140 tests) / `clippy --all-targets` / `fmt --check` propres, `curl` des lots D à I vérifiés sur la base de Paris. Mesures `oha` faites ; vérification France entière abandonnée (2026-10-08 : peu d'intérêt, on passe aux étapes 4 et 5). **Étape 3 terminée.** Écarts avec la feuille de route en bas du fichier.

## Vue d'ensemble

```text
navigateur / front Svelte
  └─ GET /api/...  ──► axum Router (état partagé : pool SQLite + table des départements)
                         ├─ extraction : Path / Query (tout en Option<String>) → params.rs → types propres ou 400
                         ├─ handler : 1 à 3 requêtes SQL (jamais une requête par ligne)
                         ├─ regroupement / distance / tri en Rust
                         └─ Json(...) ou AppError → { "error": { code, message } }
                       couches : trace → compression gzip → Cache-Control
```

| Lot | En une phrase | Fichiers principaux | Base ? |
|---|---|---|---|
| 0 | Prérequis : données réelles, vue SQL des cinémas visibles, `paris_today()` partagée | migration, `src/time.rs` | oui |
| A | Squelette du serveur : état, routeur, `serve` propre, arrêt propre | `src/api/mod.rs`, `main.rs`, `cli.rs` | non |
| B | Erreurs JSON et lecture des paramètres (fonctions pures) | `src/api/error.rs`, `src/api/params.rs` | non |
| C | Géographie : distance haversine et bounding box (fonctions pures) | `src/api/geo.rs` | non |
| D | `GET /api/cinemas` et `GET /api/cinemas/{id}` | `src/api/cinemas.rs`, `src/api/types.rs` | oui |
| E | `GET /api/meta` | `src/api/meta.rs` | oui |
| F | `GET /api/cinemas/{id}/showtimes` | `src/api/cinemas.rs` | oui |
| G | `GET /api/movies/{id}` et `GET /api/movies/{id}/showtimes` | `src/api/movies.rs` | oui |
| H | `GET /api/movies` (à l'affiche) | `src/api/movies.rs` | oui |
| I | `GET /api/search` | `src/api/search.rs` | oui |
| J | Couches HTTP : gzip, cache, logs ; mesures | `src/api/mod.rs` | – |
| K | Tests d'intégration du contrat (base de test + requêtes HTTP en mémoire) | `tests/api.rs`, `tests/fixtures/api_seed.sql` | oui |

Ordre conseillé : 0 → A → B → C → D → E → F → G → H → I → J → K. Les tests de K peuvent s'écrire **au fil de l'eau** (un test par endpoint dès qu'il marche) : c'est même mieux.

Estimation : A à C ≈ une soirée ; D à I ≈ un endpoint par séance de travail ; J et K ≈ une soirée.

---

## Décisions validées (2026-10-08)

Toutes reportées dans `API.md` (le contrat fait foi). Le tableau garde le « pourquoi » pour mémoire.

| # | Question | Décision | Lot |
|---|---|---|---|
| 1 | `Cinema.department` : `API.md` montre `"Paris"` (un nom), la base stocke `"75"` (un code) | **Renvoyer le nom**, via `docs/data/departements.csv` (déjà lu par `get_departments()`), chargé une fois au démarrage dans une `HashMap<String, String>` de l'état. Alternative : changer le contrat en `department: { code, name }` | D |
| 2 | Ordre de `GET /api/cinemas` (le contrat ne dit rien) | **Par distance si `lat`/`lng` fournis, sinon par nom** (`name_search`) | D |
| 3 | Arrondi de `distance_km` | **0,1 km** (`(d * 10.0).round() / 10.0`) : la précision des coordonnées ne justifie pas plus, et le JSON est plus court | C |
| 4 | Le filtre « cinéma visible » est écrit dans le scraper et sera écrit dans chaque requête de l'API | **Une vue SQL `visible_cinemas`** (lot 0), utilisée partout : une seule définition des 14 jours | 0 |
| 5 | `limit` de `/api/movies` : borne haute ? | **Défaut 50, max 200**, au-delà → 400 | H |
| 6 | Adresse d'écoute de `serve` | **`127.0.0.1:3000` par défaut** (en prod, nginx est devant : inutile d'exposer le port), modifiable par `--addr` ou `BIND_ADDR` | A |
| 7 | Colonnes JSON (`genres`, `directors`…) | **`sqlx::types::Json<Vec<String>>`** dans les structs de lecture (décodage automatique, une erreur de format = 500). Bonus sobriété en J.5 | D |
| 8 | `Cache-Control` sur les erreurs ? | **2xx seulement** (`public, max-age=300`) ; erreurs en `no-store` (un 500 en cache prolongerait la panne chez le visiteur) | J |
| 9 | `version=vf` en minuscules ? | **Refusé (400)** : c'est le front qui construit les URL, la tolérance ne servirait à rien | B |
| 10 | `after=9:05` sans le zéro ? | **Accepté** (ce que chrono accepte avec `%H:%M`), fixé par un test | B |
| 11 | `/api/movies/abc` | **`Path<String>` + notre parsing** → 400 au format JSON du contrat, pas le texte d'axum | G |
| 12 | CORS en dev ? | **Aucun** : proxy Vite (`/api` → `localhost:3000`), fait par Claude côté front. Le lot J.4 disparaît | J |

---

## 0. Prérequis

### 0.1 Des données réelles pour tester à la main

- [x] Lance `cargo run --release -- scrape --department 75` (≈ 107 cinémas × 3-4 dates ≈ 400 requêtes ≈ 3 min à 3 req/s). C'est aussi la vérification du lot E de l'étape 2.
- [x] Vérifie :

```sql
select count(distinct cinema_id), count(distinct movie_id), count(*) from showtimes;
select * from scrape_runs order by id desc limit 1;   -- kind = showtimes_partial, finished_at rempli
```

Référence (2026-10-08, après `--department 75` sans aucun `WARN`) : **cinémas avec séances = 81** (sur 107 visibles : les autres n'ont pas de programme), **films = 344**, **séances = 5 465** (VF 3 054, VO 457, VOST 1 954, 4 sans lien). Ces nombres servent de référence pour les `curl` des lots D à I ; ils bougent à chaque scrape.

### 0.2 La vue `visible_cinemas` (décision 4)

**Fichier** : nouvelle migration (`cargo sqlx migrate add -r visible_cinemas_view` crée `.up.sql` et `.down.sql`).

```sql
-- up
CREATE VIEW visible_cinemas AS
SELECT * FROM cinemas
WHERE lat IS NOT NULL
  AND lng IS NOT NULL
  AND updated_at >= datetime('now', '-14 days');

-- down
DROP VIEW visible_cinemas;
```

- [x] Crée la migration.
- [x] Remplace la requête de `visible_cinema_ids` (`src/showtimes/mod.rs`) par `SELECT id FROM visible_cinemas WHERE (? IS NULL OR department = ?) ORDER BY id`.
- [x] Le test `visible_cinema_query_filters_location_age_and_department` doit rester vert **sans modification** : c'est la preuve que la vue dit la même chose que l'ancienne requête.

Pourquoi une vue et pas une constante Rust : la vue marche aussi dans `sqlite3` à la main, et les macros `query!` la vérifient à la compilation.

⚠️ Piège : `datetime('now')` dans une vue est évalué **à chaque requête**, pas à la création de la vue. C'est ce qu'on veut.

### 0.3 `paris_today()` partagée

L'API a besoin du « jour ciné d'aujourd'hui » (date par défaut, `/api/meta`), exactement comme le scraper.

- [x] **Fichier** : `src/time.rs` (et `pub mod time;` dans `lib.rs`).
- [x] `pub fn paris_today() -> NaiveDate` : déplace la ligne `Utc::now().with_timezone(&Paris).date_naive()` de `showtimes::scrape` ici, et appelle-la depuis le scraper.
- [x] Ajoute aussi `pub const DATE_FORMAT: &str = "%Y-%m-%d";` et remplace les `"%Y-%m-%d"` du scraper : un seul endroit pour le format.

**C'est fini quand** : `cargo test` vert, `select count(*) from visible_cinemas` donne le même nombre que `select count(*) from cinemas where lat is not null and lng is not null and updated_at >= datetime('now','-14 days')`.

---

## A. Squelette du serveur

### A.1 Dépendances

**Fichier** : `Cargo.toml`.

```toml
tower-http = { version = "0.6", features = ["compression-gzip", "set-header", "trace"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }      # ServiceExt::oneshot, pour les tests (lot K)
http-body-util = "0.1"                                 # lire le corps d'une réponse dans les tests
```

Et dans `sqlx`, ajoute la feature `"json"` (décision 7).

- [x] `cargo build` passe.

### A.2 Le module `api`

**Fichiers** : `src/api/mod.rs` + `pub mod api;` dans `lib.rs`.

Arborescence cible (crée les fichiers vides au fur et à mesure, pas tous d'un coup) :

```text
src/api/
  mod.rs       état, routeur, couches
  error.rs     AppError (lot B)
  params.rs    lecture des paramètres de query (lot B)
  geo.rs       haversine, bounding box (lot C)
  types.rs     structs de réponse = types de API.md (lot D)
  cinemas.rs   /api/cinemas… (lots D, F)
  meta.rs      /api/meta (lot E)
  movies.rs    /api/movies… (lots G, H)
  search.rs    /api/search (lot I)
```

Ce qu'il faut dans `mod.rs` :

```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub departments: Arc<HashMap<String, String>>, // code → nom (décision 1)
}

pub fn router(state: AppState) -> Router { … }
```

Indices :

- `Router::new().route("/api/meta", get(meta::meta)).with_state(state)`.
- axum 0.8 : les paramètres de chemin s'écrivent **`/api/cinemas/{id}`** (accolades). L'ancienne syntaxe `/:id` panique au démarrage.
- `#[derive(Clone)]` est nécessaire : axum clone l'état pour chaque requête. `SqlitePool` est déjà un `Arc` en interne (le cloner ne coûte qu'un compteur), d'où l'`Arc` autour de la `HashMap` pour qu'elle aussi ne soit **jamais copiée** (`SOBRIETE.md`).
- Un handler reçoit l'état avec `State(state): State<AppState>`.
- La table des départements : `get_departments().map(|d| (d.code_insee, d.nom)).collect()`.

- [x] `AppState`, `router()` avec une seule route provisoire `/api/meta` qui renvoie `"ok"`.

### A.3 La commande `serve`

**Fichiers** : `src/cli.rs`, `src/main.rs`.

- [x] `Serve { #[arg(long, env = "BIND_ADDR", default_value = "127.0.0.1:3000")] addr: String }` (décision 6). Pour `env = …`, clap a besoin de la feature `"env"` dans `Cargo.toml`.
- [x] `serve(pool, addr)` : construit l'état, `TcpListener::bind(&addr)`, `info!(%addr, "API démarrée")`, `axum::serve(listener, router(state))`.
- [x] **Arrêt propre** : `.with_graceful_shutdown(shutdown_signal())` avec `async fn shutdown_signal() { tokio::signal::ctrl_c().await.ok(); }`. Les requêtes en cours se terminent avant la sortie. (En prod, systemd envoie `SIGTERM` : on l'ajoutera à l'étape 6 avec `tokio::signal::unix::signal(SignalKind::terminate())`.)
- [x] Supprime `root()` et le « Hello, World! ».
- [x] Déplace `serve` hors de `main.rs` (dans `api/mod.rs`, `pub async fn serve(...)`), pour que `main.rs` reste un aiguillage.

**C'est fini quand** :

```bash
cargo run -- serve &
curl -i localhost:3000/api/meta        # 200, corps "ok"
curl -i localhost:3000/nimporte        # 404
kill %1                                # le log dit que le serveur s'arrête, pas de panique
```

---

## B. Erreurs JSON et paramètres (fonctions pures)

### B.1 `AppError`

**Fichier** : `src/api/error.rs`.

Le contrat : code HTTP + `{ "error": { "code": "not_found", "message": "Cinéma introuvable" } }`, codes `bad_request` (400), `not_found` (404), `internal` (500).

```rust
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),          // message construit (ex. "Date invalide : 2026-13-01")
    NotFound(&'static str),      // message fixe ("Cinéma introuvable")
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, AppError>;
```

- [x] `impl IntoResponse for AppError` :
  - choisir `(StatusCode, code, message)` avec un `match` ;
  - pour `Internal` : **logguer** l'erreur complète (`tracing::error!(error = format!("{e:#}"), …)`) mais renvoyer au client un message générique (`"Erreur interne"`) : on ne divulgue jamais un message SQLite ;
  - construire le corps avec `serde_json::json!({ "error": { "code": code, "message": message } })` et renvoyer `(status, Json(body)).into_response()`.
- [x] `impl From<sqlx::Error> for AppError` et `impl From<anyhow::Error> for AppError` → `Internal`. Grâce à ça, `?` fonctionne directement dans les handlers sur une requête SQL.
- [x] Tests : pour chaque variante, `into_response()` donne le bon statut. Pour lire le corps dans un test : `axum::body::to_bytes(response.into_body(), usize::MAX).await` puis `serde_json::from_slice::<serde_json::Value>`.

Question à te poser : pourquoi `NotFound(&'static str)` et pas `String` ? (Indice : qui alloue, et combien de fois ?)

### B.2 Lire la query sans laisser axum répondre à notre place

Par défaut, si `Query<T>` n'arrive pas à désérialiser (`date=abc` dans un champ `NaiveDate`), axum répond **lui-même** un 400 en texte brut, en anglais : ça casse le contrat.

La solution la plus simple et la plus contrôlable : **tous les champs de query en `Option<String>`**, et la conversion faite par nos fonctions, qui renvoient nos `AppError::BadRequest`.

```rust
#[derive(Debug, Default, Deserialize)]
pub struct RawQuery {
    pub date: Option<String>,
    pub after: Option<String>,
    pub version: Option<String>,
    pub lat: Option<String>,
    pub lng: Option<String>,
    pub radius_km: Option<String>,
    pub limit: Option<String>,
    pub art_et_essai: Option<String>,
    pub q: Option<String>,
}
```

Une seule struct pour tous les endpoints : un paramètre inutile pour un endpoint est simplement ignoré (comme le fait serde avec les champs inconnus).

Dans les handlers : `Query(raw): Query<RawQuery>`. Avec des `Option<String>`, `Query` ne peut plus échouer que sur un encodage d'URL cassé, cas rarissime qu'on laisse à axum.

### B.3 Les fonctions de conversion

**Fichier** : `src/api/params.rs`. Chacune : `fn …(raw: Option<&str>, …) -> Result<…, AppError>`, **pure**, testée.

- [x] `parse_date(raw: Option<&str>, today: NaiveDate) -> ApiResult<NaiveDate>`
  - `None` → `today` ; `"2026-10-08"` → la date ; `"2026-13-01"`, `"08/10/2026"`, `""` → `BadRequest("Date invalide : …. Format attendu : YYYY-MM-DD")`.
  - `today` est un **paramètre** : la fonction ne lit pas l'horloge, donc elle se teste (même idée que `cine_dates`).
  - Indice : `NaiveDate::parse_from_str(s, DATE_FORMAT)`.
- [x] `parse_after(raw: Option<&str>) -> ApiResult<Option<NaiveTime>>`
  - `"20:00"` → `Some(20:00)` ; `"9:05"` → accepté (décision 10, un test le fixe) ; `"25:00"`, `"20h"` → `BadRequest`.
  - Indice : `NaiveTime::parse_from_str(s, "%H:%M")`.
- [x] `after_bound(date: NaiveDate, after: Option<NaiveTime>) -> Option<String>`
  - → `Some("2026-10-08T20:00:00")`, la chaîne à comparer à `starts_at` (convention `after` de `API.md` : on compare le `starts_at` **complet**, donc la séance de 00h15 le lendemain reste incluse avec `after=22:00`).
- [x] `enum VersionFilter { Vf, Vo }` + `parse_version(raw) -> ApiResult<Option<VersionFilter>>`
  - `"VF"` → `Vf`, `"VO"` → `Vo` ; `"VOST"`, `"vf"`, `"xx"` → `BadRequest` (décision 9 : `"vf"` → 400, teste-le).
  - Méthode utile : `fn as_sql(&self) -> &'static str` qui renvoie `"VF"` / `"VO"`, pour la requête (voir B.4).
- [x] `parse_bool(raw) -> ApiResult<Option<bool>>` : `"true"` / `"false"` seulement.
- [x] `struct Position { lat: f64, lng: f64 }` + `parse_position(lat, lng) -> ApiResult<Option<Position>>`
  - les deux absents → `None` ; les deux présents et valides → `Some` ;
  - un seul des deux → `BadRequest("lat et lng vont ensemble")` ;
  - hors bornes (`lat` ∉ [-90, 90], `lng` ∉ [-180, 180]) ou `NaN` → `BadRequest`. ⚠️ `"NaN".parse::<f64>()` **réussit** : vérifie `is_finite()`.
- [x] `parse_radius(raw) -> ApiResult<f64>` : défaut 15, `> 0` et `≤ 100`, sinon `BadRequest`.
- [x] `parse_limit(raw) -> ApiResult<u32>` : défaut 50, de 1 à 200 (décision 5).
- [x] `parse_search_query(raw) -> ApiResult<String>` : `trim`, puis `text::normalize`, puis au moins **2 caractères** (`chars().count()`, pas `len()` : `"é"` fait 2 octets). Renvoie la chaîne normalisée.

Astuce pour ne pas répéter le même code : une petite fonction générique

```rust
fn parse_number<T: FromStr>(raw: Option<&str>, name: &str) -> ApiResult<Option<T>>
```

qui renvoie `BadRequest(format!("{name} invalide : {s}"))`. `parse_radius` et `parse_limit` l'appellent puis vérifient les bornes.

### B.4 Filtres SQL réutilisables

Les filtres `version` et `after` reviennent dans 4 endpoints. Plutôt que de construire du SQL à la main (risque d'injection, requêtes différentes à chaque fois), on écrit la condition une fois, avec des paramètres qui la « désactivent » quand ils valent `NULL` :

```sql
-- version : ?1 = NULL (pas de filtre), 'VF' ou 'VO'
AND (?1 IS NULL OR s.version = ?1 OR (?1 = 'VO' AND s.version = 'VOST'))
-- after : ?2 = NULL ou '2026-10-08T20:00:00'
AND (?2 IS NULL OR s.starts_at >= ?2)
```

Avec les macros `query!`, les paramètres sont positionnels (`?`) : il faut lier la même valeur plusieurs fois, ou utiliser les paramètres numérotés `?1`, `?2` (SQLite les accepte, et une valeur liée une seule fois sert à chaque `?1`). Essaie `?1` : c'est plus lisible.

**C'est fini quand** : `cargo test api::params` couvre chaque cas listé, `cargo clippy --all-targets` sans warning.

---

## C. Géographie (fonctions pures)

**Fichier** : `src/api/geo.rs`.

SQLite n'a pas de fonctions géographiques. Stratégie (`API.md`, « Distance ») : un filtre **grossier** en SQL avec un rectangle (bounding box, qui profite d'un simple `BETWEEN`), puis le calcul **exact** en Rust sur les quelques dizaines de cinémas restants.

- [x] `pub fn haversine_km(a: Position, b: Position) -> f64`
  - Formule : `R = 6371.0` ; `dlat = (b.lat - a.lat).to_radians()` ; `dlng` idem ; `h = sin²(dlat/2) + cos(lat_a) · cos(lat_b) · sin²(dlng/2)` ; `d = 2R · asin(√h)`.
  - Tests : même point → `0.0` ; Paris (48.8566, 2.3522) – Lyon (45.7640, 4.8357) ≈ **392 km** (tolérance ±2 km : `(d - 392.0).abs() < 2.0`, jamais d'`assert_eq!` sur des `f64`).
- [x] `pub struct BoundingBox { min_lat, max_lat, min_lng, max_lng: f64 }` + `pub fn bounding_box(center: Position, radius_km: f64) -> BoundingBox`
  - 1° de latitude ≈ 111,32 km partout : `dlat = radius_km / 111.32`.
  - 1° de longitude rétrécit vers les pôles : `dlng = radius_km / (111.32 * center.lat.to_radians().cos())`.
  - Test : le rectangle autour de Paris avec 15 km contient un point à 14 km au nord (`lat + 14/111.32`) et pas un point à 16 km.
- [x] `pub fn round_km(d: f64) -> f64` (décision 3) : `(d * 10.0).round() / 10.0`.

Pourquoi le rectangle d'abord : sans lui, il faudrait lire les 3 100 cinémas et calculer 3 100 distances à chaque requête. Ce n'est pas énorme pour un processeur, mais c'est du travail inutile multiplié par chaque visiteur (`SOBRIETE.md` : mesurer, puis éviter le travail qui ne sert à rien).

Indice SQL (utilisé en D, G, H) :

```sql
AND c.lat BETWEEN ? AND ? AND c.lng BETWEEN ? AND ?
```

puis en Rust : calcul de la distance, `retain(|c| c.distance <= radius)`, tri.

Piège de tri : `f64` n'implémente pas `Ord` (à cause de `NaN`). Utilise `sort_by(|a, b| a.distance.total_cmp(&b.distance))`.

**C'est fini quand** : tests de `haversine_km`, `bounding_box`, `round_km` verts.

---

## D. `GET /api/cinemas` et `GET /api/cinemas/{id}`

### D.1 Les types de réponse

**Fichier** : `src/api/types.rs`. Ce sont les types TypeScript de `API.md`, traduits en Rust, avec `#[derive(Serialize)]`.

- [x] `CinemaSummary { id: String, name: String, city: Option<String>, lat: f64, lng: f64, art_et_essai: bool, distance_km: Option<f64> }`
- [x] `Cinema` = `CinemaSummary` + `address`, `postal_code`, `department`, `screens`, `seats`, `allocine_url`.
  - Indice : `#[serde(flatten)] summary: CinemaSummary` met les champs du résumé **au même niveau** dans le JSON, comme le `&` de TypeScript.
- [x] Rappel du contrat : « champs inconnus = `null` (jamais absents) ». Donc **pas** de `#[serde(skip_serializing_if = "Option::is_none")]`. Le comportement par défaut de serde (`None` → `null`) est exactement le bon.
- [x] `allocine_url` : `format!("https://www.allocine.fr/seance/salle_gen_csalle={id}.html")`. Calculé, pas stocké.

### D.2 La lecture en base

Deux options pour lire des lignes :

- `sqlx::query_as!(CinemaRow, "SELECT …")` (macro, vérifiée à la compilation, déjà utilisée dans `cinemas/mod.rs`) ;
- `sqlx::query_as::<_, CinemaRow>("SELECT …")` + `#[derive(sqlx::FromRow)]` (vérifiée à l'exécution, utilisée dans `showtimes/db.rs`).

**Prends les macros** pour l'API : toutes les requêtes sont fixes (les filtres optionnels passent par `?1 IS NULL OR …`), et une colonne mal nommée doit casser la compilation, pas la prod.

Une struct de ligne (`CinemaRow`) séparée de la struct de réponse (`CinemaSummary`) : la ligne colle à la base (`art_et_essai: i64`, `department: Option<String>` = un code), la réponse colle au contrat (`bool`, nom du département). La conversion `impl CinemaRow { fn into_summary(self, distance: Option<f64>) -> CinemaSummary }` est l'endroit unique où l'on passe de l'un à l'autre.

⚠️ Pièges des macros sqlx avec SQLite :

- sur une **vue** ou une jointure, sqlx devine parfois mal si une colonne peut être `NULL`. Force-le : `lat AS "lat!: f64"` (`!` = jamais NULL), `city AS "city?: String"` (`?` = peut être NULL). `lat` et `lng` ne sont jamais `NULL` dans `visible_cinemas` (c'est le filtre de la vue) : `!`.
- `art_et_essai` est un `INTEGER` : lis-le en `i64` puis `!= 0`, ou directement `AS "art_et_essai!: bool"` (sqlx sait convertir 0/1).
- Les macros lisent `DATABASE_URL` à la compilation. Pour la CI (étape 6), il faudra `cargo sqlx prepare` (fichiers `.sqlx/` versionnés). Pas maintenant, mais note-le.

### D.3 `GET /api/cinemas`

**Fichier** : `src/api/cinemas.rs`.

- [x] Handler `pub async fn list(State(state): State<AppState>, Query(raw): Query<RawQuery>) -> ApiResult<Json<Vec<CinemaSummary>>>`.
- [x] Paramètres : `art_et_essai` (`parse_bool`), `lat`/`lng` (`parse_position`).
- [x] Requête :

```sql
SELECT id, name, city, lat AS "lat!: f64", lng AS "lng!: f64", art_et_essai AS "art_et_essai!: bool"
FROM visible_cinemas
WHERE (?1 IS NULL OR art_et_essai = ?1)
ORDER BY name_search
```

- [x] Si position : `distance_km = Some(round_km(haversine_km(position, cinema)))` pour chaque cinéma, puis tri par distance (décision 2). Ici **pas** de bounding box : l'endpoint renvoie tous les cinémas (pour la carte), seule la distance s'ajoute.
- [x] Route : `.route("/api/cinemas", get(cinemas::list))`.

Sobriété : la réponse fait ~3 100 éléments. Construis le `Vec<CinemaSummary>` directement avec `.into_iter().map(...).collect()` (une seule allocation, à la bonne taille), pas en poussant dans un `Vec::new()` qui se réalloue.

### D.4 `GET /api/cinemas/{id}`

- [x] Handler `pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Cinema>>`.
- [x] Requête : toutes les colonnes utiles `FROM visible_cinemas WHERE id = ?`, avec `.fetch_optional(&state.pool)`.
- [x] `None` → `AppError::NotFound("Cinéma introuvable")`. Indice : `.ok_or(AppError::NotFound(…))?`.
- [x] `department` : `row.department.and_then(|code| state.departments.get(&code).cloned())` (décision 1). Si le code est inconnu de la table, `null`.
- [x] Une fonction `async fn fetch_cinema(pool, departments, id) -> ApiResult<Cinema>` réutilisable : le lot F en a besoin.

**C'est fini quand** :

```bash
curl -s localhost:3000/api/cinemas | jq length                                   # = select count(*) from visible_cinemas
curl -s 'localhost:3000/api/cinemas?art_et_essai=true' | jq length               # = … where art_et_essai = 1
curl -s 'localhost:3000/api/cinemas?lat=48.8566&lng=2.3522' | jq '.[0:3] | map({name, distance_km})'   # cinémas du centre de Paris, < 1-2 km
curl -s localhost:3000/api/cinemas/C0159 | jq                                     # department = "Paris", allocine_url correct, aucun champ absent
curl -si localhost:3000/api/cinemas/XXXX | head -1                                # 404, corps { "error": { "code": "not_found", … } }
curl -si 'localhost:3000/api/cinemas?lat=48' | head -1                            # 400 (lng manquant)
curl -si 'localhost:3000/api/cinemas?art_et_essai=oui' | head -1                  # 400
```

Et un cinéma non géocodé (`select id from cinemas where lat is null limit 1`) → 404.

---

## E. `GET /api/meta`

**Fichier** : `src/api/meta.rs`.

C'est aussi le **health check** du déploiement (étape 6) : il doit être rapide et ne jamais échouer tant que la base répond.

- [x] Struct de réponse : `Meta { today: String, last_scrape_at: Option<String>, dates_available: Vec<String>, cinema_count: i64, movie_count: i64 }`.
- [x] `today` = `paris_today()` formatée.
- [x] `last_scrape_at` (règle écrite dans `API.md`) : `finished_at` du dernier run `kind = 'showtimes'` **terminé**. La base stocke `2026-10-07 22:18:17` (UTC, format SQLite) ; le contrat veut `2026-10-07T22:18:17Z`. Fais la conversion en SQL :

```sql
SELECT strftime('%Y-%m-%dT%H:%M:%SZ', finished_at) AS "at!: String"
FROM scrape_runs
WHERE kind = 'showtimes' AND finished_at IS NOT NULL
ORDER BY finished_at DESC
LIMIT 1
```

  (`fetch_optional` : aucun run complet → `null`. Tant que tu n'as lancé que des `--department`, ce sera `null` : c'est voulu.)
- [x] `dates_available` : jours ≥ aujourd'hui ayant au moins une séance dans un cinéma visible.

```sql
SELECT DISTINCT s.date AS "date!: String"
FROM showtimes s JOIN visible_cinemas c ON c.id = s.cinema_id
WHERE s.date >= ?1
ORDER BY s.date
```

- [x] `cinema_count` = `SELECT count(*) FROM visible_cinemas` ; `movie_count` = films distincts ayant une séance ≥ aujourd'hui dans un cinéma visible.
- [x] 4 petites requêtes séquentielles, c'est très bien (quelques millisecondes). Si tu veux t'entraîner : `tokio::try_join!` les lance en parallèle sur le pool (4 connexions). Mesure avant/après (lot J) : tu verras sans doute que ça ne change rien de visible, et c'est une bonne leçon.

**C'est fini quand** :

```bash
curl -s localhost:3000/api/meta | jq
# today = date de Paris ; dates_available = select distinct date … ; last_scrape_at = null ou au format …T…Z
```

---

## F. `GET /api/cinemas/{id}/showtimes`

**Fichier** : `src/api/cinemas.rs`.

Le programme d'un cinéma pour un jour : films triés par titre, séances par heure.

### F.1 Types

**Fichier** : `src/api/types.rs`.

- [x] `MovieSummary { id: i64, title: String, poster_url: Option<String>, genres: Vec<String>, runtime_min: Option<i64>, release_date: Option<String> }`
- [x] `Showtime { id: String, starts_at: String, version: String, formats: Vec<String>, booking_url: Option<String> }`
- [x] `MovieShowtimes { movie: MovieSummary, showtimes: Vec<Showtime> }`
- [x] `CinemaShowtimesResponse { cinema: Cinema, date: String, dates: Vec<String>, movies: Vec<MovieShowtimes> }`

Colonnes JSON (décision 7) : dans la struct de **ligne**, `genres AS "genres!: Json<Vec<String>>"` ; dans la réponse, `genres: row.genres.0` (le `.0` sort le `Vec` du `Json`, sans copie).

### F.2 Une seule requête, regroupée en Rust

⚠️ Le piège classique (N+1) : une requête pour les films, puis une requête de séances **par film**. 30 films = 31 allers-retours. À la place : **une** requête qui joint séances et films, triée par titre puis heure, et on regroupe les lignes consécutives en Rust.

```sql
SELECT
    m.id AS "movie_id!: i64", m.title, m.poster_url, m.genres AS "genres!: Json<Vec<String>>",
    m.runtime_min, m.release_date,
    s.id AS "showtime_id!: String", s.starts_at, s.version, s.formats AS "formats!: Json<Vec<String>>", s.booking_url
FROM showtimes s
JOIN movies m ON m.id = s.movie_id
WHERE s.cinema_id = ?1
  AND s.date = ?2
  AND (?3 IS NULL OR s.version = ?3 OR (?3 = 'VO' AND s.version = 'VOST'))
  AND (?4 IS NULL OR s.starts_at >= ?4)
ORDER BY m.title_search, m.id, s.starts_at
```

(`m.id` dans le tri : deux films de même titre ne doivent pas mélanger leurs séances.)

Regroupement : parcours les lignes ; si `movie_id` est le même que celui du dernier élément de `movies`, pousse la séance dans ses `showtimes` ; sinon, crée un nouveau `MovieShowtimes`. Indice : `movies.last_mut()` renvoie `Option<&mut MovieShowtimes>`.

```rust
match movies.last_mut() {
    Some(last) if last.movie.id == row.movie_id => last.showtimes.push(showtime),
    _ => movies.push(MovieShowtimes { movie: …, showtimes: vec![showtime] }),
}
```

Une fonction **pure** `fn group_by_movie(rows: Vec<ShowtimeRow>) -> Vec<MovieShowtimes>` se teste sans base : fais-le (3 lignes, 2 films → 2 groupes, ordre conservé).

### F.3 Le handler

- [x] Ordre : `fetch_cinema` d'abord (404 si cinéma inconnu ou masqué, **avant** de lire les séances) ; puis paramètres (`date`, `version`, `after`) ; puis séances ; puis `dates`.
- [x] `dates` : jours ≥ aujourd'hui ayant au moins une séance **dans ce cinéma**, filtres `version`/`after` **ignorés** (le contrat le dit : c'est pour griser les jours vides dans le sélecteur).

```sql
SELECT DISTINCT date AS "date!: String" FROM showtimes WHERE cinema_id = ?1 AND date >= ?2 ORDER BY date
```

- [x] Date sans séance (ou hors `dates_available`) → **200** avec `movies: []` (convention de `API.md`), jamais 404.
- [x] Route : `.route("/api/cinemas/{id}/showtimes", get(cinemas::showtimes))`.

**C'est fini quand** (après `scrape --department 75`) :

```bash
D=$(date +%F)
curl -s "localhost:3000/api/cinemas/C0159/showtimes?date=$D" | jq '.movies | length'                      # = select count(distinct movie_id) from showtimes where cinema_id='C0159' and date='$D'
curl -s "localhost:3000/api/cinemas/C0159/showtimes?date=$D" | jq '[.movies[].showtimes[]] | length'      # = select count(*) … même filtre
curl -s "localhost:3000/api/cinemas/C0159/showtimes?date=$D&version=VO" | jq '[.movies[].showtimes[].version] | unique'   # ["VO","VOST"] ou un seul des deux
curl -s "localhost:3000/api/cinemas/C0159/showtimes?date=$D&after=20:00" | jq '[.movies[].showtimes[].starts_at] | min'  # ≥ "${D}T20:00:00"
curl -s "localhost:3000/api/cinemas/C0159/showtimes?date=2030-01-01" | jq '.movies'                       # []
```

Et compare à l'œil avec la page AlloCiné du cinéma : mêmes films, mêmes horaires.

---

## G. `GET /api/movies/{id}` et `GET /api/movies/{id}/showtimes`

**Fichier** : `src/api/movies.rs`.

### G.1 `GET /api/movies/{id}`

- [x] Type `Movie` = `MovieSummary` (flatten) + `original_title`, `synopsis`, `directors: Vec<String>`, `cast: Vec<Person>`, `countries: Vec<String>`, `production_year`, `certificate`, `backdrop_url`, `trailer_url`, `rating`, `user_rating`.
- [x] `Person { name: String, role: Option<String> }` avec `#[derive(Serialize, Deserialize)]` : `Deserialize` parce qu'on la lit depuis la colonne JSON `cast_members` (`Json<Vec<Person>>`), `Serialize` parce qu'on la renvoie.
- [x] ⚠️ Nom : la colonne s'appelle `cast_members`, le champ du contrat s'appelle `cast`.
- [x] Un film existe en base mais n'a plus de séance → on le renvoie quand même (200) : un lien partagé hier doit encore afficher la fiche. Seul un `id` inconnu donne 404 (`"Film introuvable"`).
- [x] `id` : `Path(id): Path<i64>`. Un `id` non numérique (`/api/movies/abc`) : axum répond lui-même un 400 en texte. Pour garder notre format JSON, prends `Path(id): Path<String>` et parse avec `parse_number`… (décision 11).

### G.2 Les cinémas dans un rayon (fonction partagée avec H)

C'est la brique géographique de G.3 et H. Écris-la une fois :

```rust
pub struct NearbyCinema {
    pub summary: CinemaSummary,   // distance_km rempli
    pub distance: f64,            // non arrondie, pour trier et filtrer
}

async fn cinemas_within(pool: &SqlitePool, center: Position, radius_km: f64) -> ApiResult<Vec<NearbyCinema>>
```

1. `bounding_box(center, radius_km)` ;
2. `SELECT … FROM visible_cinemas WHERE lat BETWEEN ?1 AND ?2 AND lng BETWEEN ?3 AND ?4` ;
3. distance exacte, `retain(distance <= radius_km)`, tri par distance (`total_cmp`).

Test sur base en mémoire : 3 cinémas (à 1 km, à 14 km, à 20 km du centre) avec un rayon de 15 → les deux premiers, dans l'ordre.

### G.3 `GET /api/movies/{id}/showtimes`

- [x] `lat`/`lng` **obligatoires** : `parse_position(...)?.ok_or(AppError::BadRequest("lat et lng sont obligatoires".into()))?`.
- [x] `radius_km` (défaut 15, max 100), `date`, `version`, `after`.
- [x] Film inconnu → 404 (vérifie d'abord avec une requête légère sur `movies`, qui sert aussi à remplir `movie: MovieSummary`).
- [x] `cinemas_within(...)` → liste d'IDs.
- [x] **Passer une liste d'IDs à SQLite** : sqlx ne sait pas lier un `Vec` à `IN (?)` avec SQLite. L'astuce : sérialiser la liste en JSON et utiliser `json_each`, la fonction de table JSON de SQLite :

```sql
SELECT s.cinema_id AS "cinema_id!: String", s.id AS "showtime_id!: String", s.starts_at, s.version,
       s.formats AS "formats!: Json<Vec<String>>", s.booking_url
FROM showtimes s
WHERE s.movie_id = ?1
  AND s.date = ?2
  AND s.cinema_id IN (SELECT value FROM json_each(?3))
  AND (?4 IS NULL OR s.version = ?4 OR (?4 = 'VO' AND s.version = 'VOST'))
  AND (?5 IS NULL OR s.starts_at >= ?5)
ORDER BY s.cinema_id, s.starts_at
```

  avec `?3 = serde_json::to_string(&ids)?`.
- [x] Regroupement : les séances arrivent triées par cinéma. Range-les dans une `HashMap<&str, Vec<Showtime>>` (clé = `cinema_id`), puis parcours `nearby` (**déjà trié par distance**) et garde seulement les cinémas qui ont des séances. Le tri par distance du contrat est ainsi gratuit.
- [x] `dates` : jours ≥ aujourd'hui où ce film a au moins une séance **dans le rayon** (filtres version/after ignorés) : même `json_each`, `SELECT DISTINCT date`.

**C'est fini quand** :

```bash
D=$(date +%F); M=$(sqlite3 database.db "select movie_id from showtimes where date='$D' group by 1 order by count(*) desc limit 1")
curl -s localhost:3000/api/movies/$M | jq '{title, directors, cast: .cast[0:2], genres}'
curl -s "localhost:3000/api/movies/$M/showtimes?date=$D&lat=48.8566&lng=2.3522" | jq '.cinemas | map({n: .cinema.name, d: .cinema.distance_km, s: (.showtimes | length)}) | .[0:5]'   # distances croissantes, toutes ≤ 15
curl -si "localhost:3000/api/movies/$M/showtimes" | head -1           # 400 (lat/lng obligatoires)
curl -si "localhost:3000/api/movies/1/showtimes?lat=48.85&lng=2.35" | head -1   # 404
curl -s "localhost:3000/api/movies/$M/showtimes?date=$D&lat=48.8566&lng=2.3522&radius_km=101" | jq .error.code  # "bad_request"
```

---

## H. `GET /api/movies` (à l'affiche)

**Fichier** : `src/api/movies.rs`.

La page d'accueil : les films d'un jour, triés par nombre de cinémas qui les passent.

- [x] Paramètres : `date`, `lat`/`lng` (optionnels ici), `radius_km` (défaut 15, ne sert que si position), `version`, `after`, `limit`.
- [x] Sans position : France entière (toutes les séances des cinémas visibles). Avec position : seulement les cinémas de `cinemas_within` → `json_each`.
- [x] Une seule requête d'agrégation :

```sql
SELECT
    m.id AS "id!: i64", m.title, m.poster_url, m.genres AS "genres!: Json<Vec<String>>",
    m.runtime_min, m.release_date,
    count(DISTINCT s.cinema_id) AS "cinema_count!: i64",
    count(*) AS "showtime_count!: i64",
    min(s.starts_at) AS "next_showtime!: String"
FROM showtimes s
JOIN visible_cinemas c ON c.id = s.cinema_id
JOIN movies m ON m.id = s.movie_id
WHERE s.date = ?1
  AND (?2 IS NULL OR s.cinema_id IN (SELECT value FROM json_each(?2)))
  AND (?3 IS NULL OR s.version = ?3 OR (?3 = 'VO' AND s.version = 'VOST'))
  AND (?4 IS NULL OR s.starts_at >= ?4)
GROUP BY m.id
ORDER BY cinema_count DESC, showtime_count DESC, m.title_search
LIMIT ?5
```

  `?2` = `None` sans position (pas de filtre), `Some(json)` avec.
- [x] Cas limite : position fournie mais **aucun** cinéma dans le rayon → réponds directement `movies: []` sans lancer la requête (sinon `json_each('[]')` marche aussi, mais autant ne pas travailler pour rien).
- [x] `next_showtime` = la plus petite heure ≥ `after`. Le front passera `after=<heure actuelle>` pour « la prochaine séance » ; le backend ne lit pas l'heure courante (reste testable).

Sobriété : c'est l'endpoint le plus coûteux (agrégation sur toutes les séances du jour, ~50 000 lignes pour la France). L'index `idx_showtimes_date` sert ici. Vérifie avec `EXPLAIN QUERY PLAN` dans `sqlite3` que tu vois `USING INDEX idx_showtimes_date` et pas `SCAN showtimes`. Note le temps de réponse au lot J.

**C'est fini quand** :

```bash
curl -s "localhost:3000/api/movies?date=$D" | jq '.movies[0:5] | map({t: .movie.title, c: .cinema_count, n: .showtime_count})'   # cinema_count décroissant
curl -s "localhost:3000/api/movies?date=$D&lat=48.8566&lng=2.3522&radius_km=2" | jq '.movies | length'   # moins qu'avec radius_km=15
curl -s "localhost:3000/api/movies?date=$D&limit=3" | jq '.movies | length'                              # 3
curl -s "localhost:3000/api/movies?date=$D&limit=500" | jq .error.code                                   # "bad_request"
```

---

## I. `GET /api/search`

**Fichier** : `src/api/search.rs`.

- [x] `q` : `parse_search_query` (normalisée, ≥ 2 caractères, sinon 400).
- [x] ⚠️ **`LIKE` et les jokers** : dans `LIKE`, `%` et `_` sont des jokers. Si l'utilisateur tape `100%`, il ne faut pas que `%` matche tout. Échappe-les avant de construire le motif :

```rust
fn like_pattern(q: &str) -> String  // "100%" → "%100\%%"   (puis `LIKE ?1 ESCAPE '\'`)
```

  Remplace `\` par `\\`, `%` par `\%`, `_` par `\_`, puis entoure de `%`. Teste-la (fonction pure).
- [x] Films : uniquement ceux qui ont au moins une séance ≥ aujourd'hui dans un cinéma visible ; les « plus diffusés d'abord » = nombre de séances.

```sql
SELECT m.id AS "id!: i64", m.title, m.poster_url, m.genres AS "genres!: Json<Vec<String>>", m.runtime_min, m.release_date
FROM movies m
JOIN showtimes s ON s.movie_id = m.id
JOIN visible_cinemas c ON c.id = s.cinema_id
WHERE s.date >= ?1
  AND m.title_search LIKE ?2 ESCAPE '\'
GROUP BY m.id
ORDER BY count(*) DESC
LIMIT 8
```

  Le titre original n'a pas de colonne `_search` : on cherche sur le titre français seul. Chercher aussi « The Batman » quand le titre français diffère demanderait une migration `original_title_search` + son remplissage dans `upsert_movie` : note-le en idée, ne le fais pas maintenant.
- [x] Cinémas : nom **ou** ville (`"montreuil"` doit trouver le Méliès).

```sql
SELECT id, name, city, lat AS "lat!: f64", lng AS "lng!: f64", art_et_essai AS "art_et_essai!: bool"
FROM visible_cinemas
WHERE name_search LIKE ?1 ESCAPE '\' OR city_search LIKE ?1 ESCAPE '\'
ORDER BY name_search
LIMIT 8
```

  `distance_km` = `null` (pas de position dans `/api/search`).
- [x] Réponse : `{ "movies": [...], "cinemas": [...] }`.

Pourquoi c'est rapide sans index plein texte : `LIKE '%…%'` lit toute la table, mais `cinemas` fait 3 100 lignes et les films à l'affiche quelques centaines. C'est de l'ordre de la milliseconde. Un index FTS5 ne se justifierait qu'à des centaines de milliers de lignes : ne l'ajoute pas sans mesure.

**C'est fini quand** :

```bash
curl -s 'localhost:3000/api/search?q=cine%20cite' | jq '.cinemas | map(.name)'     # trouve les « Ciné Cité » (sans accents dans la requête)
curl -s 'localhost:3000/api/search?q=montreuil' | jq '.cinemas | map(.name)'       # contient le Méliès
curl -s 'localhost:3000/api/search?q=ÉTÉ' | jq '.movies | map(.title)'             # casse et accents ignorés
curl -s 'localhost:3000/api/search?q=%25' | jq .error.code                         # "%" seul = 1 caractère → "bad_request"
curl -s 'localhost:3000/api/search?q=a_' | jq '.cinemas | length'                  # "_" n'est pas un joker : peu ou pas de résultats
```

---

## J. Couches HTTP et mesures

**Fichier** : `src/api/mod.rs`, dans `router()`.

Une couche (`layer`) enveloppe tous les handlers : c'est là qu'on met ce qui vaut pour **toutes** les réponses.

- [x] **J.1 Logs** : `.layer(TraceLayer::new_for_http())`. Avec `RUST_LOG=info,tower_http=debug`, chaque requête est loggée avec sa durée.
- [x] **J.2 Gzip** : `.layer(CompressionLayer::new())`. Ne compresse que si le client envoie `Accept-Encoding: gzip`. Vérifie le gain sur `/api/cinemas` :

```bash
curl -s localhost:3000/api/cinemas | wc -c                                   # taille brute
curl -s -H 'Accept-Encoding: gzip' localhost:3000/api/cinemas | wc -c        # taille compressée (API.md annonce ≈ 90 Ko pour ~3 100 cinémas)
```

- [x] **J.3 Cache** : `Cache-Control: public, max-age=300` sur les réponses (contrat). `SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"))`. Décision 8 : seulement sur les **2xx**, et `no-store` sur les erreurs. Indice : `SetResponseHeaderLayer::overriding` accepte aussi une closure qui reçoit la réponse (`|res: &Response<_>| …` → `Option<HeaderValue>`) : regarde `res.status().is_success()`. Autre option : mettre `no-store` directement dans `AppError::into_response`, et `if_not_present(public, max-age=300)` en couche. Teste les deux cas (200 et 404).
- ~~**J.4 CORS**~~ : supprimé (décision 12). En dev, le front passe par le proxy Vite ; en prod, même domaine.
- [ ] **J.5 Bonus sobriété** (optionnel) : les colonnes `genres`, `formats`… sont déjà du JSON en base. Les décoder en `Vec<String>` puis les ré-encoder en JSON, c'est un aller-retour inutile. `Box<serde_json::value::RawValue>` permet de les recopier telles quelles dans la réponse. Mesure d'abord (J.6) : si le gain n'est pas visible, garde `Json<Vec<String>>`, plus simple et qui valide le format.
- [x] **J.6 Mesures** (faites sur la base Paris, `oha` compris ; pas de re-mesure France entière, décision du 2026-10-08) → `SOBRIETE.md` (toujours en `--release`) :
  - mémoire au repos du processus `serve` : `ps -o rss= -p $(pgrep -f 'backend serve')` (en Ko) ;
  - temps de réponse de chaque endpoint : `curl -s -o /dev/null -w '%{time_total}\n' URL` (5 fois, prends la médiane) ;
  - débit sous charge, si tu installes `oha` (`brew install oha`) : `oha -z 10s -c 20 "localhost:3000/api/movies?date=$D"` → requêtes/s et latence p99.
  - Ordre de grandeur attendu : quelques Mo de RSS au repos, quelques ms par requête. Si `/api/movies` dépasse ~50 ms, regarde `EXPLAIN QUERY PLAN`.

**C'est fini quand** : `curl -sI -H 'Accept-Encoding: gzip' localhost:3000/api/cinemas` montre `content-encoding: gzip` et `cache-control: public, max-age=300`, `curl -sI localhost:3000/api/cinemas/XXXX` montre `cache-control: no-store`, et les mesures sont dans `SOBRIETE.md`.

---

## K. Tests d'intégration du contrat

**Fichiers** : `tests/fixtures/api_seed.sql`, `tests/api.rs` (ou des tests dans chaque module `api/*.rs`, au choix ; `tests/` teste l'API comme un client extérieur, c'est le plus parlant).

### K.1 Une base de test qui contient tous les pièges

**Fichier** : `tests/fixtures/api_seed.sql`, inséré après les migrations dans une base `sqlite::memory:`.

Contenu minimal (avec des dates **fixes**, ex. `2026-10-08` et `2026-10-09`, et des `updated_at = datetime('now')` pour que les cinémas soient visibles) :

| Donnée | Pourquoi |
|---|---|
| `PARIS1` à 1 km du centre de Paris, art et essai, département `75` | cas nominal, filtre `art_et_essai`, nom du département |
| `PARIS2` à 10 km | tri par distance, rayon |
| `LYON` | hors rayon de Paris, présent sans position |
| `NOGEO` (`lat` NULL) avec des séances | doit être **invisible partout** (404, aucune séance) |
| `OLD` (`updated_at = '2000-01-01 00:00:00'`) avec des séances | idem |
| Film A, séances VF + VOST, dont une à `2026-10-09T00:15:00` pour la date `2026-10-08` | `after=22:00` doit la garder ; elle n'apparaît **pas** le `2026-10-09` |
| Film B, une seule séance VO | `version=VO` renvoie VO + VOST ; `version=VF` l'exclut |
| Film C sans aucune séance | `/api/movies/{id}` = 200, absent de `/api/movies` et de la recherche |
| Un cinéma au nom accentué (`Ciné Cité`) et une ville `Montreuil` | recherche sans accents, par ville |

### K.2 Faire une requête HTTP sans ouvrir de port

```rust
use tower::ServiceExt; // pour .oneshot()

let app = backend::api::router(state);
let response = app
    .oneshot(Request::builder().uri("/api/cinemas/PARIS1").body(Body::empty()).unwrap())
    .await
    .unwrap();
assert_eq!(response.status(), StatusCode::OK);
let bytes = response.into_body().collect().await.unwrap().to_bytes(); // http_body_util::BodyExt
let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
```

`oneshot` consomme le routeur : reconstruis-en un par requête (c'est bon marché, l'état est fait d'`Arc`), ou écris une petite fonction `async fn get(state: &AppState, uri: &str) -> (StatusCode, serde_json::Value)` qui fait tout ça. Avec elle, chaque test fait 3 lignes.

### K.3 Les tests à écrire (un par ligne)

- [x] `/api/cinemas` : `LYON`, `PARIS1`, `PARIS2` seulement (ni `NOGEO` ni `OLD`) ; `distance_km` = `null` sans position.
- [x] `/api/cinemas?lat=…&lng=…` : ordre `PARIS1`, `PARIS2`, `LYON` ; distances arrondies à 0,1.
- [x] `/api/cinemas/NOGEO` et `/api/cinemas/OLD` → 404 avec `error.code = "not_found"`.
- [x] `/api/cinemas/PARIS1` : `department = "Paris"`, **tous** les champs présents (vérifie qu'une clé `null` existe bien : `json.get("screens").is_some()`).
- [x] `/api/cinemas/PARIS1/showtimes?date=2026-10-08&after=22:00` : contient la séance de 00h15.
- [x] `/api/cinemas/PARIS1/showtimes?date=2026-10-09` : ne contient **pas** la séance de 00h15.
- [x] `…?version=VO` : versions ⊂ {VO, VOST} ; `…?version=VF` : uniquement VF.
- [x] `…?date=2030-01-01` → 200, `movies: []`.
- [x] `/api/movies?date=2026-10-08` : film C absent, tri par `cinema_count`.
- [x] `/api/movies/{C}` → 200 ; `/api/movies/999` → 404.
- [x] `/api/movies/{A}/showtimes` sans `lat` → 400 ; avec Paris et `radius_km=5` → seulement `PARIS1`.
- [x] `/api/search?q=cine cite` et `?q=montreuil` trouvent le bon cinéma ; `?q=a` → 400.
- [x] Paramètres invalides (`date=2026-13-01`, `after=25:00`, `version=VOST`, `version=vf`, `art_et_essai=oui`, `limit=0`, `/api/movies/abc`) → 400 `bad_request` au format JSON.
- [x] Une erreur porte `cache-control: no-store`, une réponse 200 `public, max-age=300`.
- [x] `/api/meta` : `dates_available` ne contient que des dates ≥ `today` (attention, `today` dépend de l'horloge dans ce test : vérifie la forme, pas les valeurs exactes).

**C'est fini quand (fin de l'étape 3, à cocher dans `PLAN.md`)** :

- `cargo test` vert, `cargo clippy --all-targets` sans warning, `cargo fmt --check` propre ;
- les 8 endpoints de `API.md` répondent sur une base réelle (après un `scrape` complet lancé par toi), avec les `curl` des lots D à I ;
- mesures (RSS au repos, temps de réponse des 8 endpoints, débit de `/api/movies`) notées dans `SOBRIETE.md`.

Ensuite : étape 4 (TMDB) côté backend, et je branche le front sur la vraie API (étape 5) dès que D et F marchent.

---

## Aide-mémoire axum 0.8

| Besoin | Code |
|---|---|
| Route avec paramètre | `.route("/api/cinemas/{id}", get(handler))` |
| Lire l'état | `State(state): State<AppState>` |
| Lire un paramètre de chemin | `Path(id): Path<String>` |
| Lire la query | `Query(raw): Query<RawQuery>` (dernier argument ou avant-dernier, peu importe ; **le corps de requête**, lui, doit être le dernier, mais on n'en a pas) |
| Répondre en JSON | `Ok(Json(valeur))` avec `valeur: impl Serialize` |
| Type de retour | `ApiResult<Json<T>>` = `Result<Json<T>, AppError>` |
| Erreur « pas trouvé » | `.fetch_optional(&pool).await?.ok_or(AppError::NotFound("…"))?` |
| Ajouter une couche | `.layer(...)` après les routes (les couches s'appliquent aux routes déclarées **avant**) |
| Lancer | `axum::serve(listener, app).with_graceful_shutdown(signal).await` |

Erreur de compilation fréquente : « the trait `Handler<_, _>` is not implemented ». Causes habituelles : un argument du handler n'est pas un extracteur, le type de retour n'implémente pas `IntoResponse`, ou le futur n'est pas `Send` (un `MutexGuard` de `std` ou un `Rc` gardé à travers un `.await`). `#[axum::debug_handler]` au-dessus du handler (feature `"macros"` d'axum) donne un message beaucoup plus clair.

---

## Ce qui a été fait différemment de la feuille de route (2026-10-08)

À relire en priorité : ce sont les endroits où le code ne suit pas le texte ci-dessus.

- **0.2** : le test `visible_cinema_query_filters_location_age_and_department` **a dû changer**. La vue exige aussi `lng IS NOT NULL`, l'ancienne requête du scraper ne regardait que `lat`, et le test insérait des cinémas sans `lng`. La fixture donne maintenant une `lng`, et un cinéma `NO_LNG` vérifie la nouvelle règle (c'est celle du contrat).
- **A.1** : `tower-http` 0.7 (déjà dans ton `Cargo.toml`) au lieu de 0.6, sans la feature `cors` (décision 12). `clap` a reçu la feature `env`.
- **A.3** : seulement `ctrl_c` pour l'arrêt propre ; `pkill` (SIGTERM) tue le processus sans le log « API arrêtée ». SIGTERM reste pour l'étape 6.
- **B.3** : `Position` est dans `geo.rs` (pas `params.rs`). `parse_value` (une chaîne) + `parse_number` (une `Option`) au lieu d'une seule fonction, pour éviter des `.expect()` dans `parse_position` et `parse_movie_id`.
- **H** : `ORDER BY count(DISTINCT s.cinema_id) DESC, count(*) DESC` au lieu de `ORDER BY cinema_count` : avec les macros, l'alias s'appelle littéralement `cinema_count!: i64`, SQLite répond « no such column ».
- **J.3** : les deux options combinées : `AppError::into_response` pose `no-store`, et la couche `SetResponseHeaderLayer::if_not_present` met `public, max-age=300` sur les 2xx (et `no-store` sur le reste, pour le 404 de route inconnue).
- **Route inconnue** : `.fallback` renvoie un 404 JSON (`"Route introuvable"`) au lieu du 404 vide d'axum.
- **J.5** (`RawValue`) : pas fait, les mesures ne le justifient pas (≤ 3 ms hors `/api/cinemas`).
- **K.1** : séances en **2099** au lieu du 08/10/2026, pour que les filtres `date >= aujourd'hui` (dates, recherche, meta) ne cassent pas les tests dès demain.
- **Département** : `get_departments()` ignore Mayotte (976, pas de chemin AlloCiné) et logge un `WARN` au démarrage de `serve`. Aucun cinéma 976 en base aujourd'hui ; si un jour il y en a, son `department` sera `null`.

Questions pour toi (la partie Rust non triviale) :

1. Dans `movies::showtimes` (fin de la fonction), pourquoi `by_cinema.remove(&cinema.summary.id)` plutôt que `by_cinema.get(&cinema.summary.id).cloned()` ? Que coûterait la seconde version ?
2. Dans `cinemas::list`, le tri se fait sur la distance **arrondie** avec `sort_by` (stable). Qu'est-ce que la stabilité apporte ici, et que se passerait-il avec `sort_unstable_by` ?
3. `AppState` est cloné à chaque requête : qu'est-ce qui est réellement copié ?
