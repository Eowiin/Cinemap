# Suivi de l'étape 1 (temporaire)

Feuille de route pas à pas pour le backend, à consulter sans rouvrir la conversation.
Le **quoi** et le **pourquoi** sont ici, avec des indices ; le **comment**, c'est toi qui l'écris.
À supprimer (ou à fondre dans `PLAN.md`) une fois l'étape 1 terminée.

Dernière mise à jour : 2026-10-02. **Prochaine étape : D1 (un seul `reqwest::Client`).**

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

1. Un seul `reqwest::Client`, créé une fois et réutilisé (pourquoi ? regarde la doc de `Client` sur le pool de connexions). User-Agent navigateur (voir `SOURCES.md` §4).
2. `fetch(url) -> anyhow::Result<String>` : `.get(url).send().await?`, puis **`.error_for_status()?`** (sinon un 404/403 passe pour un succès), puis `.text().await?`.
3. Pour un département : page 1 → `page_count` → pages 2..=N. Commencer en **séquentiel**, avec une petite pause entre les requêtes (`tokio::time::sleep`). La concurrence viendra à l'étape 2.
4. Tous départements : `HashMap<String, TheaterListing>` (ou `BTreeMap`) indexé par ID pour **dédoublonner** (la page IDF 83093 recoupe les départements IDF). Logguer le nombre de doublons ignorés.
5. Pour l'instant, se contenter de logguer / compter. L'écriture en base (upsert dans `cinemas`) + géocodage + CNC = suite de l'étape 1 dans `PLAN.md`.
6. Vérifier : Paris / IDF d'abord, ~85 cinémas attendus dans Paris intra-muros, aucun ID en double.

## Prompt pour reprendre dans une nouvelle conversation

```text
On reprend la réécriture de Cinemap (branche rewrite). Lis d'abord : docs/PLAN.md,
docs/SUIVI.md (feuille de route détaillée de l'étape 1, à jour), docs/API.md (contrat
figé : toute modif passe d'abord par ce fichier), docs/SOURCES.md,
docs/SOBRIETE.md (réflexes ressources, à enrichir pendant les relectures). Puis regarde backend/.

Rôles : le backend Rust, c'est MOI qui l'écris pour apprendre. Tu me guides et relis mon
code, sans écrire l'implémentation sauf si je te le demande explicitement. Le frontend
(Svelte + Vite + TS + MapLibre/OpenFreeMap, PWA), c'est toi (étape 5).

Méthode : des INDICES, pas des solutions (questions, où regarder : fichier, ligne, doc,
méthode, macro), un point à la fois. Donne la réponse seulement si je la demande ou si je
bloque encore après 2 indices. Avant chaque indice, relis le code et lance-le (cargo build /
cargo run) pour vérifier ce qui est déjà réglé.

Où j'en suis : voir la section de SUIVI.md dont les cases ne sont pas cochées.
Mets à jour les cases et notes de docs/PLAN.md et docs/SUIVI.md au fur et à mesure.
Réponds en français. Ne commite pas sans me demander.
```
