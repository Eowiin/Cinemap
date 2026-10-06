# Suivi de l'étape 1 (temporaire)

Feuille de route pas à pas pour le backend, à consulter sans rouvrir la conversation.
Le **quoi** et le **pourquoi** sont ici, avec des indices ; le **comment**, c'est toi qui l'écris.
À supprimer (ou à fondre dans `PLAN.md`) une fois l'étape 1 terminée.

Dernière mise à jour : 2026-10-05. **Étape 1 terminée. Prochaine étape : étape 2, lot A — voir [`ETAPE2.md`](ETAPE2.md).**

## Déjà fait ✅

- Compilation OK, erreurs propagées avec `?` / `anyhow`
- Code INSEE en `String` (2A/2B, « 01 »)
- CSV des départements embarqué dans le binaire (`include_str!`), lu avec `csv` + serde
- SQLite : WAL, `foreign_keys`, `busy_timeout`, pool de 4 connexions commenté
- Migration `down` des index corrigée
- Logs via `tracing`, `RUST_LOG` prioritaire, sinon `info`

## A. Finir le nettoyage de `import_cinemas` (presque fini)

- [x] **Mayotte** : `allocine_code: Option<String>`. Dans la boucle, un `let Some(code) = … else { warn!(…); continue; };` (ou un `match`) pour la signaler et passer.
  - Vérifier : `cargo run -- import-cinemas` doit afficher un `WARN` pour Mayotte.
  - 2026-09-30 : OK (« 100 chargés, 1 ignoré »). Petit plus possible : nommer le département dans le `warn!` (aujourd'hui « no allocine_code » sans dire lequel).
  - Piège : avec `Option<String>`, une case vide devient-elle `None` ou `Some("")` ? Teste. Si c'est `Some("")`, cherche `deserialize_with` / `csv::invalid_option` dans la doc du crate `csv`.
- [x] **Ne pas jeter l'erreur** : `match row { Ok(d) => …, Err(e) => warn!(…, "{e}") }`. L'erreur de `csv` contient déjà la position → le compteur `line` devient peut-être inutile.
- [x] **Récapitulatif** : un seul `info!` après la boucle (« N départements chargés, M ignorés »).
- [ ] **`serve()`** (✅ fait) : renvoyer `anyhow::Result<()>`, remplacer `.unwrap()` et `let _ =` par `?`. Même chose pour `scrape()` (et plus de `Ok(scrape())` dans `main`) ← reste à faire, clippy le signale (« passing a unit value »).
- [ ] **Warnings** : `cargo build` sans aucun warning, puis `cargo clippy` et `cargo fmt`.
  - 2026-09-30 : build → 1 warning (`code_insee`/`nom` jamais lus, disparaîtra quand on s'en servira) ; clippy → +`return` inutile dans `db.rs`, +`Ok(scrape())` ; `cargo fmt` pas encore passé.
- [x] (Optionnel) `env::var("DATABASE_URL").context("…")?` (trait `anyhow::Context`) à la place du `if let … else`.

## B. Découpage en modules (en cours)

2026-09-30 : fichiers créés (`lib.rs`, `cli.rs`, `db.rs`, `cinemas/`). Restent : `main.rs` redéclare les modules avec `mod …` au lieu de `use backend::…` (ils sont compilés deux fois) ; la lecture du CSV est encore dans `cinemas/mod.rs` au lieu de `departments.rs`. Attention : dans `.env`, `DATABASE_URL` est commenté → `cargo run` échoue sans la variable.

Orchestration (question du 2026-09-30, « pourquoi lire tout le CSV puis reboucler ? ») : il n'y a qu'**une** boucle sur les départements. `departments.rs` expose une fonction qui fournit les départements valides (au choix un `Vec` ou un itérateur), `mod.rs` boucle dessus et, pour chacun, télécharge → `allocine::parse_department_page` → dédoublonne.

**Décision (2026-10-01) : itérateur.** Le CSV est embarqué à la compilation, il ne peut pas changer entre deux exécutions : pas besoin de tout valider avant le réseau, un test unitaire le garantit une fois pour toutes.

- [x] `departments.rs` : `include_str!` déplacé (chemin ajusté) + `pub fn …() -> impl Iterator<Item = Department>` qui loggue et saute les lignes invalides (`into_deserialize`, `filter_map`)
  - 2026-10-01 : fait. Leçon : les itérateurs sont paresseux, la closure du `filter_map` tourne pendant la boucle de l'appelant, pas dans `get_departments` → les compteurs vivent là où on consomme l'itérateur.
- [ ] `cinemas/mod.rs` : `import_cinemas` ne fait plus que boucler sur cet itérateur (pour l'instant un `debug!` + le compteur « N chargés »)
  - 2026-10-01 : boucle écrite mais vide → affiche « 0 chargés, 0 ignorés ». `ignored` ne peut pas être compté ici tant que `get_departments` filtre.
- [x] `main.rs` : `use backend::…` au lieu des `mod …` (2026-10-01)
- [x] Test dans `departments.rs` : le CSV donne bien 100 départements valides (2026-10-01)

Objectif : `main.rs` ne fait plus que parser la CLI, initialiser les logs et appeler la bonne fonction.

```
backend/src/
├── main.rs        # CLI → appel de la sous-commande, rien d'autre
├── lib.rs         # déclare les modules (pub mod …) : permet les tests d'intégration
├── cli.rs         # struct Cli + enum SubCommands
├── db.rs          # establish_connection, run_migrations
└── cinemas/
    ├── mod.rs     # import_cinemas (orchestration)
    ├── departments.rs  # struct Department + lecture du CSV
    └── allocine.rs     # parsing HTML des pages département (étape C)
```

Indices :
- Rust Book, chapitre 7 (« Managing Growing Projects… »), surtout 7.5 « Separating Modules into Different Files ».
- Question à te poser : pourquoi un `lib.rs` en plus de `main.rs` ? Que peut importer un test dans `tests/` : un binaire ou une bibliothèque ? Dans `main.rs`, on y accède avec `use backend::…`.
- Visibilité : ce qui n'est pas `pub` n'est pas visible hors du module. Ne mets `pub` que là où le compilateur le réclame.
- `include_str!` est relatif au **fichier source** : si tu le déplaces dans `cinemas/departments.rs`, le chemin change.
- Vérifier : `cargo build` sans warning, et `cargo run -- import-cinemas` donne la même sortie qu'avant.

## C. Parser une page département AlloCiné hors ligne ✅

✅ Terminée le 2026-10-02 : `parse_department_page` et `page_count` dans `cinemas/allocine.rs`, 3 tests verts sur la fixture (50 cinémas, C0159, 15 pages, 1 page sans pagination).

2026-10-01 : un premier `reqwest::get` a été écrit dans `allocine.rs` (étape D en avance). Conseil : séparer **téléchargement** et **parsing** ; écrire d'abord `parse_department_page(&str)` testée sur la fixture, le réseau ensuite. Rappel du déroulé (question du 2026-10-01) :
- `import-cinemas` = référentiel des cinémas (rare). `scrape` = **séances** (chaque nuit, étape 2) : le parsing des cinémas n'a rien à faire sous `scrape`.
- Les pages ne sont **pas** enregistrées sur disque : téléchargées → parsées en mémoire → jetées. Seule la fixture est gardée, pour les tests.
- Base : on écrit **à la fin** de `import-cinemas`, une fois tout dédoublonné, en une transaction (upsert). Le parsing ne donne que `id`, `name`, `address` (+ le département connu par la boucle) ; `postal_code`, `city`, `insee_code`, `lat`/`lng` viendront du géocodage.

2026-10-02 : `parse_department_page` renvoie 50 cinémas avec `id` + `name` correctement décodés (sélecteur `span[data-theater]` + `serde_json`). Reste : l'adresse (passer par les cartes, deux niveaux), transformer l'appel via `scrape` en vrai `#[test]`, `page_count`.

2026-10-02 (suite) : parsing en deux niveaux (cartes → span + adresse) OK, test vert (50). Mais toute la logique est **dans le test** : à remettre dans `parse_department_page`, le test ne fait qu'appeler + vérifier. Code postal via `rsplit(' ').nth(1)` faux sur 2 adresses (« … 75015 Paris 15e arrondissement » → « 15e ») ; le géocodage le fournira de toute façon.

2026-10-02 (fin) : ✅ `parse_department_page(&str) -> Vec<Cinema>` hors du test, `address: Option<String>` (trimée), `let … else` + `warn!`, test : 50 cinémas + C0159 (nom décodé, adresse exacte). Reste : `page_count`.

Constats sur la fixture : 50 cartes `.theater-card` (attention, `theater-card-extra` est une autre classe), un seul `data-theater` et un seul `<address>` par carte, liens de pagination `?page=2` … `?page=10` + `?page=15`.

Principe : on télécharge **une fois** une vraie page, on la garde dans le dépôt, et on teste le parsing dessus sans réseau.

1. Fixture :
   ```sh
   mkdir -p backend/tests/fixtures
   curl -sL -A 'Mozilla/5.0' 'https://www.allocine.fr/salle/cinema/departement-83093/' \
     -o backend/tests/fixtures/departement-83093-p1.html
   ```
   (Vérifié le 2026-09-30 : ça répond 200, ~360 Ko.)
   ⚠️ Ces commandes se lancent depuis la **racine du dépôt**. Depuis `backend/`, retirer le préfixe `backend/`. Le code de sortie 6 de curl veut dire « nom de domaine non résolu » (réseau/DNS), réessayer suffit.
   - [x] Fixture téléchargée le 2026-10-01 (200, 362 Ko, 50 occurrences de `data-theater`). Ouvre-la dans un éditeur et repère les `data-theater` et les `<address>` (voir `SOURCES.md` §1).
2. Une struct pour le résultat, par exemple `TheaterListing { id, name, address }`, et une fonction **pure** `parse_department_page(html: &str) -> …` : pas de réseau, pas de base, juste du texte en entrée. C'est ce qui la rend testable.
3. Indices `scraper` :
   - `Html::parse_document`, `Selector::parse`, `.select(&selector)`, `.value().attr("data-theater")`, `.text()`.
   - Chercher d'abord les cartes (`.theater-card`), puis **dans chaque carte** l'attribut et l'adresse. Pourquoi est-ce plus sûr que de chercher tous les `data-theater` puis tous les `<address>` séparément ?
   - L'attribut contient du JSON (`{"id":"C0159","name":"UGC Ciné Cité …"}`) : `scraper` décode déjà `&quot;`, et `serde_json::from_str` vers une petite struct `#[derive(Deserialize)]` décode les `é`.
   - Attention : `data-theater` apparaît peut-être plusieurs fois par carte. Compte-les et regarde où.
   - Adresse : `.text()` renvoie des morceaux, pense à les assembler et à nettoyer les espaces.
4. Pagination (toujours hors ligne) : une fonction `page_count(html) -> u32`.
   - Piège vérifié : la pagination n'affiche qu'une **fenêtre** de liens (`?page=2` … `?page=10`, puis `?page=15`). Il ne faut donc pas compter les liens mais prendre **le plus grand numéro**. Et s'il n'y a aucun lien `?page=` ?
5. Tests unitaires dans le même fichier : `#[cfg(test)] mod tests { … }`, fixture chargée avec `include_str!("../../tests/fixtures/…")` (chemin relatif au fichier source !).
   - Que tester : nombre de cinémas sur la page (compte-le à la main dans la fixture), présence de `C0159` avec le bon nom **décodé** (« UGC Ciné Cité Les Halles ») et une adresse non vide, `page_count` = 15.
   - `cargo test`. Rust Book chapitre 11.

## D. Réseau et pagination (en cours)

2026-10-02 (reprise) : `cargo build` → 2 warnings (`result` inutilisé, `id`/`name` jamais lus), `cargo test` → 4 verts. `import_cinemas` boucle déjà sur les départements et appelle `get_cinemas_from_department`, qui utilise `reqwest::get` (sans pause, sans User-Agent) : ne pas lancer `import-cinemas` en l'état (≈100 requêtes d'affilée). À vérifier plus tard : `migrations/20260929121908_create_scrape_runs.sql` n'a pas le suffixe `.up` alors que son `.down.sql` existe.

2026-10-02 (D1) : `src/client.rs` avec `build_client()` (User-Agent, Referer), créé une fois dans `import_cinemas` et passé en `&Client` ✅. Puis `client.get(url).send()` à la place de `reqwest::get` → **D1 terminé**. Déjà en vue pour la suite : `Accept: application/json` alors qu'on télécharge du HTML ; `HeaderMap`/`HeaderValue` importés via `axum::http` plutôt que `reqwest::header` ; boucle de pagination `for page in 1..=max_page` (D3).

1. ✅ Un seul `reqwest::Client`, créé une fois et réutilisé (pourquoi ? regarde la doc de `Client` sur le pool de connexions). User-Agent navigateur (voir `SOURCES.md` §4).
2. `fetch(url) -> anyhow::Result<String>` : `.get(url).send().await?`, puis **`.error_for_status()?`** (sinon un 404/403 passe pour un succès), puis `.text().await?`.
   - 2026-10-02 (D2) : `.error_for_status()` ajouté, `match` → `warn!` + on continue. Reste : `e` jamais affiché (warning), et la politique : un 403/429 ne doit pas être « ignoré puis on passe au département suivant » (SOBRIETE §Réseau : s'arrêter tout de suite). `fetch` pas encore extrait.
   - 2026-10-02 : ✅ choix final : tout remonter avec `?` après `error_for_status()` (« le moins prise de tête »). Défendable : commande manuelle et rare, mieux vaut échouer bruyamment. Manque juste le contexte (quel département / quelle page) → `anyhow::Context`.
3. Pour un département : page 1 → `page_count` → pages 2..=N. Commencer en **séquentiel**, avec une petite pause entre les requêtes (`tokio::time::sleep`). La concurrence viendra à l'étape 2.
4. Tous départements : `HashMap<String, TheaterListing>` (ou `BTreeMap`) indexé par ID pour **dédoublonner** (la page IDF 83093 recoupe les départements IDF). Logguer le nombre de doublons ignorés.
5. Pour l'instant, se contenter de logguer / compter. L'écriture en base (upsert dans `cinemas`) + géocodage + CNC = suite de l'étape 1 dans `PLAN.md`.
6. Vérifier : Paris / IDF d'abord, ~85 cinémas attendus dans Paris intra-muros, aucun ID en double.

2026-10-02 (lot D3-D6) : ✅ boucle `while` + pause seulement entre deux pages ; ✅ `fetch(client, url)` extraite ; ✅ `get_cinemas_from_department(code, name, client) -> Vec<Cinema>` ; ✅ `HashMap<String, Cinema>` par ID cinéma + `insert(..).is_some()` pour les doublons ; ✅ `Accept` retiré, imports via `reqwest::header` ; ✅ clippy propre (reste `name` jamais lu, normal tant qu'on n'écrit pas en base). Reste :
- [x] `with_context` ne s'applique qu'au résultat de `.text()` : les `?` de `send` / `error_for_status` sortent avant, sans contexte. Et le message ne contient pas l'URL.
- [x] `duplicates` est déclaré dans la boucle (remis à 0 à chaque département) et l'`info!` est dans la boucle : un récapitulatif final unique (+ éventuellement un `debug!` par département).
- [ ] `cargo fmt` (diff restant dans `main.rs`, toujours là le 2026-10-02)
- [x] Test réseau sur 83093 seul (`take`/`filter` temporaire), puis toute l'IDF : ~85 cinémas dans Paris, aucun doublon restant.
- Remarque sobriété : chaque page est parsée deux fois (`page_count` + `parse_department_page`). Négligeable devant les 3 s de pause : exemple de « mesurer d'abord », pas à changer.

2026-10-02 (fin D) : ✅ contexte avec l'URL sur chaque `?` de `fetch`, compteur de doublons global + un seul `info!`, test réseau OK (d'après le propriétaire, chiffres non notés). **Section D terminée.**

## E. Écrire les cinémas en base

Ordre du pipeline (une seule commande `import-cinemas`) : scraping → **écriture** → géocodage → CNC → rapport. On écrit tôt pour que les étapes suivantes puissent être relancées et testées sans re-scraper.

- [ ] `import_cinemas(pool: &SqlitePool)` : `main` a déjà le pool, il suffit de le passer (emprunt).
- [ ] Upsert dans **une** transaction : `pool.begin()`, une requête par cinéma sur `&mut *tx`, `tx.commit()`. SQL : `INSERT … ON CONFLICT(id) DO UPDATE SET name = excluded.name, …`. Question : quelles colonnes **ne pas** écraser au ré-import (pense à `lat`/`lng` déjà géocodés si l'adresse n'a pas changé) ?
- [ ] `name_search` est `NOT NULL` : fonction pure `normalize(&str) -> String` (minuscules, sans accents). Indice : décomposition Unicode **NFD** puis retirer les « combining marks » (crate `unicode-normalization`), ou le crate `deunicode`. Elle resservira pour `city_search`, le CNC et l'API (étape 3) → module à part (`src/text.rs` ?), avec des tests : « Ciné Cité » → « cine cite », « ÉCRAN » → « ecran ».
- [ ] `updated_at` : laisser SQLite le remplir (`datetime('now')` dans la requête) plutôt qu'ajouter un crate de dates.
- [ ] `department` : **ne pas** prendre le code de la boucle (83093 n'est pas un département). Il viendra du code INSEE à l'étape F (2 premiers caractères, 3 pour l'outre-mer `97x`, `2A`/`2B` en Corse).
- [ ] `sqlx::query!` vérifie le SQL **à la compilation** contre la base de `DATABASE_URL` (pratique, mais il faut la base migrée) ; `sqlx::query` + `.bind()` ne vérifie rien. Choisis, et lis la doc de `query!` sur le mode hors ligne (`cargo sqlx prepare`) pour la CI plus tard.
- Vérifier : `sqlite3 database.db "select count(*), count(distinct id) from cinemas"` ; relancer l'import ne crée pas de doublon ; le warning « `name` never read » a disparu.

## F. Géocodage en masse (API Adresse)

Vérifié le 2026-10-02 : `https://api-adresse.data.gouv.fr/search/csv/` répond toujours, mais le service officiel est désormais la Géoplateforme : `POST https://data.geopf.fr/geocodage/search/csv` (mêmes paramètres, même CSV de réponse). Colonnes utiles de la réponse : `id` (renvoyée telle quelle), `latitude`, `longitude`, `result_score`, `result_postcode`, `result_city`, `result_citycode`, `result_status`.

- [ ] Même méthode qu'en C : **fixture d'abord**. Envoie à la main un petit CSV (3-4 cinémas, dont un sans adresse et un à l'adresse bizarre) avec `curl -X POST -F data=@petit.csv -F columns=adresse https://data.geopf.fr/geocodage/search/csv`, garde la réponse dans `tests/fixtures/`, et écris le parsing testé hors ligne.
- [ ] Construire le CSV **en mémoire** : `csv::Writer::from_writer(Vec::new())`, colonnes `id,adresse`, puis `into_inner()`. Lire depuis la base les cinémas à géocoder (adresse non nulle, `lat` nulle ou adresse modifiée).
- [ ] Envoi multipart : feature `multipart` de reqwest, `reqwest::multipart::Form` + `Part::bytes(…).file_name("cinemas.csv")` (sans nom de fichier le serveur refuse souvent), champ `columns=adresse`. Réutilise ton `Client` (un `Referer` AlloCiné n'y gêne pas, mais demande-toi si `build_client` doit le mettre par défaut pour tout le monde).
- [ ] Réponse : `csv::Reader` + struct `#[derive(Deserialize)]` avec `#[serde(rename = "result_score")]` ; les champs peuvent être **vides** quand rien n'est trouvé → `Option<f64>` (rappel du piège Mayotte de la section A).
- [ ] Score < 0.5 ou statut ≠ `ok` → `warn!` avec l'ID, le nom et l'adresse ; on garde quand même le résultat ? (à toi de trancher, note-le).
- [ ] Mise à jour en base en une transaction : `lat`, `lng`, `geocode_score`, `postal_code`, `city`, `city_search`, `insee_code`, `department`.
- [ ] Taille : ~2 000 lignes en une requête, c'est dans les limites documentées (vérifie la limite de taille du fichier dans la doc de l'API).
- Vérifier : `select count(*) from cinemas where lat is null` ; ouvrir 2-3 coordonnées dans une carte ; C0159 → `75101`, département `75`.

### Relecture E-F (2026-10-03)

Fait : pool passé à `import_cinemas`, upsert `ON CONFLICT` avec remise à `NULL` de `lat`/`lng`/`geocode_score` si l'adresse change (correct : dans un `SET`, SQLite évalue tout sur l'ancienne ligne), `normalize` via `deunicode`, `query!`, CSV en mémoire, Referer AlloCiné déplacé dans `fetch`. Build OK (2 warnings), 4 tests verts.

Mais **la base est vide** (`select count(*) from cinemas` → 0) :
- [ ] Pas de `tx.commit()` : une transaction abandonnée est annulée (`Drop` de `Transaction`). Faire **deux** transactions courtes (upsert, puis mises à jour du géocodage), pas une seule qui reste ouverte pendant l'appel HTTP (verrou d'écriture SQLite tenu pendant le réseau).
- [ ] `build_cinemas_csv(pool)` lit avec une autre connexion que la transaction → ne voit pas les lignes non validées → CSV vide.
- [ ] Formulaire multipart : champs `resourceName` / `FileData` → l'API répond **HTTP 500** (vérifié avec curl). Attendu : `data` (le fichier) + `columns=adresse` (sinon l'API concatène toutes les colonnes, ID compris).
- [ ] `GeoData` : une adresse vide ou introuvable revient avec tous les champs vides et `result_status = skipped` (vérifié) → `f32` non optionnels = échec de désérialisation, et le `?` de `get_geodata` arrête tout. → `Option<…>`, et `warn!` + `continue` sur une ligne invalide.
- [ ] `result_status` et score jamais utilisés (warning) : `warn!` si score < 0.5 ou statut ≠ `ok`.
- [ ] `insee_code` reçoit le code du **département** de la boucle (et 83093 n'en est pas un) : la colonne attend le code **commune** = `result_citycode`. `department` n'est jamais rempli → le déduire de `result_citycode` (2 car., 3 pour `97x`, 2A/2B). `Cinema::code_insee` devient inutile.
- [ ] `build_cinemas_csv` : `address.unwrap()` panique sur un cinéma sans adresse ; ne sélectionner que `address IS NOT NULL AND lat IS NULL`. `id.unwrap()` : SQLite autorise `NULL` dans un `TEXT PRIMARY KEY`, d'où l'`Option` ; regarder la syntaxe `id as "id!"` de `query!`.
- [ ] `f32` pour lat/lng/score : SQLite stocke de toute façon un `REAL` 8 octets → `f64`, aucun gain à `f32` ici (note SOBRIETE).
- [ ] Pas de fixture ni de test pour le géocodage, ni pour `normalize` (prévu en F1 / E).
- [ ] Clippy : `.values()` au lieu de `for (_, cinema) in &map` ; `&String` → `&str` ; import `anyhow` inutile ; `cargo fmt`.
- [ ] Découpage : `cinemas/mod.rs` contient tout (upsert, CSV, géocodage) → `cinemas/geocode.rs`, `src/text.rs` (normalize), `mod.rs` n'orchestre plus que les étapes.
- Vérifier : `sqlite3 backend/database.db "select count(*), count(lat), count(insee_code), count(department) from cinemas"`.

### Relecture E-F, 2e passe (2026-10-03)

✅ Corrigé : deux transactions courtes avec `commit`, formulaire `data` + `columns=adresse`, `"id!"`/`"address!"`, filtre « pas encore géocodé », `Option<f64>`, `warn!` structuré (score < 0.5 / statut ≠ ok), `insee_code` = `result_citycode`, `department_from_insee` (renvoie un `&str` emprunté, testée), modules `cinemas/geocode.rs` et `text.rs`, fixture + 11 tests verts, build et clippy propres. Reste `cargo fmt`.

Petites remarques : l'upsert remet `insee_code` à `NULL` si l'adresse change, mais pas `department`/`postal_code`/`city`/`city_search` ; `get_geodata` échoue sur tout le lot si une seule ligne est invalide (choix assumé, testé).

**Premier import complet (lancé en `--release`) : échec après 13 min.** `connection error / timed out` sur `departement-83140?page=3` (Hérault, 34e département sur 100 ; l'URL répond 200 juste après). **Cause confirmée** (`pmset -g log`) : le Mac s'est mis en veille de 22:16:29 à 22:18:23, la connexion en cours a été coupée. En local, lancer l'import avec `caffeinate -i …` pour empêcher la veille. Comme tout reste en mémoire jusqu'à la fin, **la base est restée vide** : 13 min de scraping perdues. Mesures quand même : 788 s réels (dont ~2 min de veille), 0,84 s CPU, RSS max 22 Mo (→ SOBRIETE.md). Durée estimée d'un import complet ≈ 40 min.

- Priorité revue à la baisse (la cause était la veille), mais reste utile : sur le serveur, l'import tournera seul la nuit et une vraie coupure réseau arrivera un jour.
- [x] Réessayer les erreurs **passagères** (timeout, connexion, 5xx) quelques fois avec une pause croissante ; ne **pas** réessayer 403/429/404. Où : dans `fetch`. Indices : `reqwest::Error::is_timeout()`, `is_connect()`, `status()` ; `StatusCode::is_server_error()` ; une boucle `for attempt in 1..=3` + `tokio::time::sleep` (backoff 5 s, 15 s, 45 s par ex.). Logguer chaque nouvel essai en `warn!`.
- [x] Délais explicites sur le `Client` : `ClientBuilder::connect_timeout` et `timeout` (par défaut, **pas** de timeout global dans reqwest). Valeurs raisonnables : 10 s / 30 s.
- [x] Ne plus tout perdre : écrire **par département** (un upsert en transaction courte après chaque département). Le dédoublonnage reste assuré : par la `HashMap` pour le compteur, et par `ON CONFLICT(id)` en base. Question : le géocodage reste-t-il en fin d'import, ou après chaque département ? (Penser : une seule requête CSV pour tout, c'est l'intérêt de l'API en masse.)
- [ ] (Option) Reprise : sous-commande ou option `--from <code_insee>` pour repartir d'un département. Utile, mais seulement si l'import échoue encore.
- Vérifier : relancer l'import complet (~40 min, en tâche de fond), puis les requêtes `count(*)` de la relecture précédente.

2026-10-03 (soir) : ✅ `fetch_with_retries` (délais injectés 5/15/45 s, `is_retryable` : 5xx + erreurs de transport, pas 4xx), `connect_timeout` 10 s / `timeout` 30 s, retry interne de reqwest désactivé (`retry::never()`, évite de réessayer deux fois), upsert après chaque département avec contexte « Import du département … », tests : serveur HTTP factice (nouvel essai sur 5xx, pas sur 404) et base en mémoire (un département validé survit à l'échec du suivant). 17 tests verts, clippy et fmt propres. Remarque : `is_decode()` dans `is_retryable` (un corps mal encodé ne se répare pas en réessayant) et `is_request()` est large ; à garder en tête si on voit des essais inutiles dans les logs.

### Premier import complet réussi (2026-10-03)

Résultat : **3 024 cinémas**, 3 017 avec adresse, **3 004 géocodés** (13 adresses non trouvées, 7 sans adresse), 146 scores < 0.5 (~5 %, surtout des adresses vagues : « ZAC … », « Centre commercial … », « Salle des fêtes … »), 100 départements. C0159 → `75101`, département `75`, (48.8619, 2.3466) ✅.

**Mais Paris : 10 cinémas seulement** (attendu ~85-100). Cause : la page 83093 ne contient que les grands multiplexes parisiens. Paris a une page ville `ville-115755` (107 entrées, 20 par page), voir `SOURCES.md` §1.

- [x] Pour Paris, scraper `https://www.allocine.fr/salle/cinema/ville-115755/` : colonne `allocine_path` dans le CSV (`departement-83169`, `ville-115755`), sans branchement sur le département dans le code.
- [x] Retirer `83093` : deux imports complets successifs (sans puis avec l’agrégat) trouvent chacun **3 121 IDs uniques**. Sans agrégat : **0 doublon** ; avec : **265 doublons**, aucun ID supplémentaire. Les snapshots adresse/géocodage après les deux passages sont identiques. Configuration finale : Paris ville + départements, sans agrégat.
- [x] Ajouter un `/` avant `?page=` (sinon chaque requête fait un `301` en plus : deux allers-retours au lieu d'un).
- [x] Fixture `ville-115755-p1.html` + test : 20 cinémas, `page_count` ≥ 6 (même parseur, vérifie qu'il marche tel quel).
- Vérification complète (2026-10-03) : **3 121 cinémas, 3 100 géocodés, 148 scores < 0,5, 105 dans le 75**, aucun ID en double. `C0159` reste `75101` / `75` / `(48.861942, 2.346567)`. 22 tests passent, Clippy et formatage propres. Durées observées : 404,66 s sans agrégat, 462,62 s avec (les variations réseau empêchent d’attribuer tout l’écart à l’agrégat). Les deux variantes ont été compilées depuis le même code, avec une ligne supplémentaire temporaire dans le CSV pour `departement-83093` ; cette ligne a été retirée de la configuration finale.
- Configuration appliquée : `allocine_path` dans le CSV, chemin utilisé tel quel par le scraper (aucun branchement sur le code 75). Fixture réelle téléchargée ; test : 20 cinémas et au moins 6 pages.
- [x] Réimport sans agrégat : **105 cinémas géocodés dans le 75**, 107 entrées dans la source ville. 97 nouveaux cinémas ; les 3 004 anciens géocodages sont inchangés (comparaison des snapshots avant/après). 110 adresses envoyées : les 97 nouvelles + les 13 anciennes introuvables, retentées par le filtre actuel.
- Deux entrées parisiennes hors du compteur : `W7508` (MK2 Grand Palais), adresse introuvable ; `C0127` (mk2 Bibliothèque x Centre Pompidou), adresse longue envoyée à l’API mais résultat à Marseille (`13213`, score ~0,301), signalé par le warning de score faible. Pas de correction manuelle appliquée dans ce lot.
- À surveiller : d'autres grandes villes pourraient avoir le même problème (département incomplet). Le croisement CNC (G) le révélera : un département avec beaucoup de cinémas CNC non croisés = page AlloCiné incomplète.

### Relecture du lot Paris (2026-10-03, fait avec Codex)

✅ Validé : `allocine_path` dans le CSV (aucun `if` sur le 75), `listing_url` testée, 83093 retiré sur mesure (0 vs 265 doublons, mêmes 3 121 IDs), fixture Paris réelle, `is_retryable` qui remonte la chaîne `Error::source()` jusqu'à l'`io::Error` (répond aux remarques `is_decode`/`is_request`), test « l'upsert sans changement d'adresse garde le géocodage ». Vérifié de mon côté : 22 tests, clippy, fmt, base = 3 121 / 3 100 géocodés / 105 dans le 75.

**F-bis : C0127 n'est pas un cas isolé.** Requête sur la base : 90 cinémas dont le `postal_code` géocodé n'apparaît pas dans l'adresse AlloCiné, dont **~49 placés dans un autre département** (C0127 Paris → Marseille, W9161 Essonne → Vaucluse, P8517 Marne → Indre…), tous avec un score < 0.5. Sur la carte, ces cinémas seraient à des centaines de km. La politique actuelle « score faible = `warn!` mais on garde » est donc trop permissive.

- **Critère = cohérence du code postal, pas le score** (analyse du 2026-10-03) : sur 148 scores < 0.5, 121 sont dans la bonne commune (adresse juste vague) ; à l'inverse 27 scores ≥ 0.5 sont dans un autre département (ex. W0730 « 6 Rue de Montceau-les-Mines 62440 Harnes » → Montceau-les-Mines, 0.83 : le nom de la rue pris pour la ville). Le score mesure la ressemblance du texte, pas la justesse du lieu → n'enregistrer que les résultats cohérents avec le code postal de l'adresse ; le score reste une info (précision), pas un filtre. Attention : un contrôle par département a des faux positifs (Corse : `20100` vs `2A`), le filtre par code postal non.
- [x] Extraire le code postal de l'adresse AlloCiné (le **dernier** groupe de 5 chiffres ; rappel : `rsplit(' ').nth(1)` échouait sur « 75015 Paris 15e arrondissement »). Fonction pure + tests dans `text.rs`.
- [x] L'envoyer à l'API comme filtre : colonne `postcode` dans le CSV + champ multipart `postcode=postcode`. Vérifié le 2026-10-03 : avec ce filtre, C0127 ne renvoie **plus Marseille** mais aucun résultat (mieux vaut pas de point qu'un point faux).
- [x] Repli pour les adresses non trouvées : deuxième lot avec seulement « code postal + ville » → position au centre de la commune (`result_type = municipality`). Question : faut-il le signaler (score, ou colonne) pour que le front sache que la position est approximative ? Si ça touche `API.md`, en discuter d'abord.
- [ ] Nettoyer l'adresse avant envoi : 11 adresses contiennent un saut de ligne (texte d'accès type « accès en face de… »). Garder la première ligne ? Voir `str::lines`.
- [x] Re-géocoder l'existant : le filtre actuel (`lat IS NULL`) ne reprendra jamais C0127. Élargir à `geocode_score < 0.5` (ou ajouter une sous-commande `geocode` qui ne refait que cette étape, sans les ~7 min de scraping).
- Vérifier (plus tard, demande un appel à l'API mais pas de scraping si sous-commande dédiée) : la requête de contrôle ci-dessous doit tomber proche de 0.
  ```sql
  select count(*) from cinemas
  where postal_code is not null and instr(address, postal_code) = 0
    and address not like '% ' || department || '%';
  ```
- 2026-10-05 : ✅ F-bis commité (`242284e`) : filtre code postal, second lot de repli (adresse sans filtre pour les CEDEX, puis code postal + ville limité à `municipality`) accepté seulement si le département et le lieu concordent, colonne `geocode_type` (migration + `API.md`), sous-commande `geocode --all` sans scraping. Base : requête de contrôle ci-dessus = **0** (au lieu de ~90) ; 13 cinémas sans position ; types : 1 500 `housenumber`, 1 419 `street`, 82 `locality`, 107 `municipality`.
- [ ] Reste ouvert (mineur) : 11 adresses contiennent toujours un saut de ligne en base ; aucune n'est fausse au contrôle, mais le texte d'accès part dans la requête et s'affichera tel quel côté front.

## G. Enrichissement CNC (XLSX)

2026-10-05 : lecture hors ligne de la fixture CNC opérationnelle (`cinemas/cnc.rs`) : feuille 2025, **2 060 établissements**, tests sur le Balzac et UGC Opéra. `fauteuils` peut être vide → `Option<f64>` en entrée, `Option<i64>` en sortie. Les 42 tests passent (les tests HTTP locaux nécessitent une exécution hors sandbox), build OK. Restent avant validation du parseur : validation des nombres pour `cnc_id`/`screens`, traitement explicite des valeurs `AE` inconnues, erreur au lieu du `unwrap` sur une feuille vide, nettoyage des logs, Clippy (4 warnings) et formatage. Téléchargement en production, rapprochement et écriture CNC encore à faire.

2026-10-05 (relecture) : ✅ parseur validé. `latest_year_sheet` passe par `filter_map` (année, nom) puis `max_by_key` : il renvoie le nom d'origine, et une erreur s'il n'y a pas de feuille d'année. Valeur `AE` inconnue → erreur avec la valeur ; `Vec::with_capacity(range.height())` ; tous les `DEPCOM` font 5 caractères (aucun zéro initial perdu, vérifié par un test). Messages d'erreur passés en français. Build, Clippy, fmt OK, 42 tests verts.

Voir `SOURCES.md` §3 (feuille la plus récente, en-têtes ligne 5, `NAutoC`, `NomEtab`, `Ecrans`, `fauteuils`, `DEPCOM`, `AE`).

- [x] Télécharger le XLSX en mémoire (`.bytes()`), l'ouvrir avec `calamine` sans fichier : `open_workbook_from_rs` + `std::io::Cursor`. Fixture : garde le XLSX dans `tests/fixtures/` (quelques centaines de Ko) pour tester hors ligne.
- [x] Choisir la feuille : `sheet_names()`, garder celle dont le nom est la plus grande année (`parse::<u16>()`).
- [x] Lignes : sauter les 4 premières, lire par **nom d'en-tête** plutôt que par index (les colonnes bougent d'une année à l'autre). Regarde si `calamine` sait désérialiser avec serde (`RangeDeserializerBuilder`). Attention aux types : un nombre Excel peut arriver en `f64`, `DEPCOM` peut perdre son zéro initial (« 1053 » au lieu de « 01053 »).
- [x] Croisement : regrouper le CNC par `DEPCOM` (`HashMap<String, Vec<…>>`), puis pour chaque cinéma, candidats de la même commune, comparés sur `normalize(nom)` avec une similarité (crate `strsim`, par ex. `jaro_winkler` ou `normalized_levenshtein`). Seuil à choisir en regardant les cas réels (log `debug!` des paires et scores). Un seul candidat dans la commune : l'accepter même avec un score moyen ?
- [x] Mettre à jour `cnc_id`, `screens`, `seats`, `art_et_essai` (une transaction).
- Vérifier : taux de croisement (on vise > 85 %) et lire une dizaine de non-croisés pour ajuster.
- 2026-10-05 (écrit par Claude à ta demande, à relire) : sous-commande `enrich-cnc` (aussi appelée en fin d'`import-cinemas`). `fetch_with_retries` déplacé dans `client.rs` ; il prend une closure qui construit la requête (l'appelant ajoute `REFERER` pour AlloCiné, un délai de 120 s pour le CNC). Croisement par commune ; similarité = max(`jaro_winkler`, part des mots du nom le plus court présents dans l'autre), après retrait des mots génériques (« cinéma », « le », « mega »…) et `st` → `saint`. Seuil 0,85. Attribution du meilleur score d'abord, un établissement CNC par cinéma au plus. Écriture : remise à zéro puis `UPDATE` dans une seule transaction.
  - Piège trouvé : le CNC note **Lyon et Marseille en commune entière** (`69123`, `13055`), alors que le géocodage renvoie l'arrondissement (`69381`, `13201`…). Paris est par arrondissement des deux côtés. → `commune_code()` ramène Lyon/Marseille à la commune.
  - Résultat : **1 729 croisés / 2 060 CNC** (84 %), dont **1 712 / 1 945 salles fixes (88 %)** ; les itinérants (115) sont rarement sur AlloCiné. `cnc_id` sans doublon (1 729 distincts), 1 147 Art et Essai. Paris : 73 cinémas AlloCiné sur 107 croisés, seuls **6** établissements CNC parisiens restent sans correspondance (`PARNASSIEN` / « Sept Parnassiens », `PATHE LA GEODE` / « La Géode - IMAX »…). Les autres non-croisés AlloCiné parisiens sont des musées, instituts, festivals, absents du CNC.
  - 128 non-croisés CNC n'ont **aucun** cinéma AlloCiné dans leur commune. À Lyon, AlloCiné n'a que 10 cinémas : manquent `CINEDUCHERE`, `Lumière Fourmi`, `CINEMA BELLECOMBE`, `CINEMA OPERA`, `LE CINEMA`, `SAINT DENIS`. À Marseille, `ALHAMBRA` et `GYPTIS` sont absents de la base. **Même symptôme que Paris** : la page département AlloCiné est incomplète pour ces deux villes → ajouter leurs pages ville (`allocine_path`), comme `ville-115755`.
  - Égalités corrigées : « UGC Ciné Cité Lyon Part-Dieu » prenait `UGC CINE CITE` (inclusion complète, score 1,0) au lieu de `UGC CINE CITE PART-DIEU`. À score égal, le `jaro_winkler` brut départage (test dédié). 8 tests en plus, 50 au total.
  - Mesure (`--release`, `/usr/bin/time -l`) : 0,85 s réel, 0,08 s CPU, **28 Mo** de mémoire max (contre 22 Mo pour le scraping AlloCiné).
  - [x] Relu et commité (`5ac8d3a`). Réponses aux questions (2026-10-06) : 2 (`E0505`, `matches` emprunte `cnc`) et 3 (`FnMut`, un élément à la fois) justes. 1 à préciser : les clés empruntent `CncCinema.insee_code` (le slice `cnc`), pas `LocatedCinema` ; pour Lyon/Marseille, `commune_code` renvoie un littéral `&'static str`, qui peut être raccourci à la durée de vie de l'entrée. Le nom normalisé, lui, n'existe nulle part avant l'appel : il faut une `String` qui en est propriétaire.
  - [x] CNC indisponible pendant `import-cinemas` → `warn!` au lieu d'un échec (les cinémas sont déjà enregistrés, les anciennes données CNC restent : la remise à zéro n'a lieu qu'après un téléchargement réussi). `enrich-cnc` seul échoue toujours franchement.
  - [x] **Lyon et Marseille vérifiés le 2026-10-06** : la page ville liste des cinémas absents de la page département. Lyon `ville-113315` : 17 cinémas, dont **7 absents** de `departement-83196` (CinéDuchère, Lumière La Fourmi, Bellecombe, Le Cinéma Opéra, Le Cinéma, Ciné Saint-Denis, Aquarium Ciné-Café). Marseille `ville-87914` : 16, dont **6 absents** de `departement-83188` (Alhambra, Gyptis, Château de la Buzine, La Cinémathèque, Mucem, Villa Méditerranée). Les 8 autres pages ville liées depuis ces deux départements (Aix, Arles, Vaulx-en-Velin…) n'apportent **aucun** cinéma. Hypothèse : seules les trois villes à arrondissements (Paris, Lyon, Marseille) sont concernées. → Deux lignes ajoutées au CSV (`69,Lyon,ville-113315`, `13,Marseille,ville-87914`), **sans changer le code** (doublons gérés par `ON CONFLICT(id)`). Fixture `ville-113315-p1.html` + test ; test du CSV : 102 sources, 100 codes distincts.


## H. Rapport et vérification

- [x] En fin d'`import-cinemas`, un `info!` (ou quelques lignes) : nombre de cinémas, non géocodés, score faible, non croisés CNC, doublons ignorés. Le plus simple : des `SELECT count(*) … WHERE …` sur la base.
- 2026-10-06 (écrit par Claude) : `import_report` (une requête, `count(col)` + `sum(condition)`, testée sur une base en mémoire) puis un `info!` « Import des cinémas terminé » : cinémas, sans position, position commune, score faible, croisés / non croisés CNC, **absents de cet import** (`updated_at` antérieur au début de l'import : cinémas plus listés par AlloCiné, fermés ?), doublons ignorés. Le comptage des doublons garde seulement les IDs (`HashSet<String>`) au lieu des cinémas entiers. Chiffres actuels de la base : 3 121 cinémas, 13 sans position, 107 position commune, 155 score < 0,5, 1 729 croisés CNC.
- [x] **Import complet du 2026-10-06** (09:15 → 09:22, lancé par toi) : **3 134 cinémas** (+13 : exactement les 7 de Lyon et les 6 de Marseille), 13 sans position, 107 position commune, 155 scores < 0,5, **1 739 croisés CNC** (+10, aucun `cnc_id` en double), 0 absent de cet import. Lyon : 17 cinémas, Marseille : 16 (27 croisés CNC sur 33). Requête de contrôle F-bis toujours à 0. Mesure `/usr/bin/time -l` non relevée.
- [x] Cinémas plus listés (décidé le 2026-10-06, règle dans `API.md` « Cycle de vie des cinémas ») : pas de suppression sur une seule absence. Masqués de l'API après **14 jours** sans être vus (`updated_at`, sans nouvelle colonne), **supprimés après 60 jours** à la fin d'un import réussi (`purge_unseen_cinemas`, testée à la seconde près ; séances supprimées par cascade). Le rapport affiche `supprimes`. Base actuelle : 0 cinéma concerné.
- [ ] Étape 3 (API) : appliquer le filtre des 14 jours dans **toutes** les requêtes de `serve` (comme `lat IS NOT NULL`).
- [ ] Vérif manuelle Paris : `select count(*) from cinemas where department = '75'` (~85), aucun ID en double → cocher la dernière ligne de l'étape 1 dans `PLAN.md`.
- [ ] Mesure (SOBRIETE.md) : `/usr/bin/time -l cargo run --release -- import-cinemas` → durée totale (dominée par les pauses) et mémoire max. Note les chiffres.

## Étape 2

Feuille de route détaillée : [`ETAPE2.md`](ETAPE2.md).

## Prompt pour reprendre dans une nouvelle conversation

```text
On reprend la réécriture de Cinemap (branche rewrite). Lis d'abord : docs/PLAN.md,
docs/SUIVI.md (étape 1, terminée), docs/ETAPE2.md (feuille de route
détaillée de l'étape 2 : la ligne « Prochain lot » en haut et les cases), docs/API.md (contrat figé :
toute modif passe d'abord par ce fichier), docs/SOURCES.md, docs/SOBRIETE.md
(réflexes ressources, à enrichir pendant les relectures). Puis regarde backend/
et `git status` (du travail peut ne pas être commité).

Rôles : le backend Rust, c'est MOI qui l'écris pour apprendre (il m'arrive de
déléguer un lot à Codex : relis-le comme mon code et pose-moi 2-3 questions sur
les passages Rust non triviaux). Tu me guides et relis, sans écrire
l'implémentation sauf si je te le demande explicitement. Le frontend
(Svelte + Vite + TS + MapLibre/OpenFreeMap, PWA), c'est toi (étape 5).

Méthode : des INDICES, pas des solutions (questions, où regarder : fichier, ligne,
doc, méthode, macro), par LOTS de plusieurs étapes numérotées avec une
vérification à la fin. Donne la réponse seulement si je la demande ou si je bloque
encore après 2 indices. Avant chaque relecture, lance cargo build / clippy / fmt
--check / test et interroge la base (sqlite3 backend/database.db) pour vérifier
avec des chiffres. Ne lance PAS d'import complet (~7 min de scraping AlloCiné)
sans me demander : note-le pour que je le lance (avec `caffeinate -i` sur le Mac).

Où j'en suis : étape 1 terminée (3 134 cinémas, CNC, rapport, cycle de vie).
Étape 2 (séances) : lot A de docs/ETAPE2.md (fixtures JSON, structs serde). Mets à jour les cases et notes de docs/PLAN.md et
docs/SUIVI.md au fur et à mesure. Réponds en français.
Tu peux commiter et pousser quand je le demande (pas avant).
```
