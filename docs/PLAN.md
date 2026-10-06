# Plan de la réécriture

Document de suivi. **À mettre à jour à chaque étape terminée** (cocher, noter les décisions).

Branche de travail : `rewrite` → mergée dans `main` à la toute fin (le push sur `main` déclenche le déploiement).

## Objectif

Site public (d'abord pour moi et mes potes) qui répond à :

- **Qu'est-ce qui passe en ce moment ?** Accueil « à l'affiche », près de moi (Paris par défaut), pour trouver des idées.
- **Où voir tel film ?** Recherche d'un film → cinémas qui le passent, sur la carte et en liste.
- **C'est quoi ce film ?** Fiche film : synopsis, casting, réalisateur, genres, durée, bande-annonce.
- **À quelle heure ?** Séances d'un film par cinéma (VF/VO/VOST, formats, lien de réservation), triées par distance ou par heure.

Périmètre : toute la France, **Paris / Île-de-France en priorité** (qualité des données vérifiée d'abord là). Utilisable en site web et en **PWA** installable.

Contraintes : gratuit (aucune source de données payante), hébergé sur le VPS existant (https://cinemap.eowinstudio.com).

## Décisions

| Sujet | Décision | Pourquoi |
|---|---|---|
| Backend | **Rust** : axum, tokio, sqlx (SQLite), reqwest, scraper, serde, clap, tracing | Le propriétaire apprend Rust ; le backend est **écrit par lui**, Claude guide et relit seulement |
| Frontend | **Svelte + Vite + MapLibre GL** (tuiles OpenFreeMap), PWA via `vite-plugin-pwa` | Léger, réactif, carte vectorielle fluide sur mobile. **Écrit par Claude** |
| Référentiel cinémas | **AlloCiné** (liste par département) + géocodage **API Adresse** + enrichissement **CNC** | Plus aucun rapprochement de noms pour les séances (cause n°1 des bugs de l'ancien site). OSM abandonné |
| Infos films | AlloCiné (déjà dans la réponse des séances) + **TMDB** en complément (bande-annonce, image de fond, note) | TMDB gratuit pour usage non commercial, en français. OMDb écarté (anglais seulement, 1 000 req/jour) |
| Rafraîchissement | Scrap de toute la France chaque nuit, J → J+2 (J+6 le mercredi), concurrent avec limite de débit (~3 req/s) | ~30-40 min au lieu de ~6 h. Stratégie hors IDF à revoir si besoin (pas prioritaire) |
| Base | SQLite en mode WAL | Un seul serveur, lecture majoritaire |
| Déploiement | **Binaire + systemd + nginx**, pas de Docker. Scrapers via un timer systemd. Build dans GitHub Actions, envoi par SSH | Un binaire Rust n'a pas de dépendances à embarquer, le VPS n'a besoin ni de Rust ni de Node |
| Outillage | Rust : `cargo fmt`, `clippy`, `cargo test`. Front : prettier, eslint, vitest. CI GitHub Actions | Minimum raisonnable |

## Pourquoi l'ancien site ne marchait pas (diagnostic 2026-09-25)

- 717 cinémas partageaient 270 identifiants AlloCiné (rapprochement par noms trop permissif) : tous les UGC de Paris affichaient les séances d'un seul UGC, idem pour les MK2 et les Pathé.
- L'API séances est paginée (15 films par page) et seule la première page était lue : il manquait plus de la moitié des films des gros cinémas.
- Environ 310 cinémas en double issus d'un ancien import (Paris : 166 entrées pour environ 85 vrais cinémas).
- Côté front : date calculée en UTC, filtre VO non recalculé au changement de date, plantages quand un cinéma est hors du filtre actif, réponses asynchrones arrivant dans le désordre.

## Documents de référence

- [`API.md`](API.md) : contrat backend ↔ frontend et schéma SQLite. **Toute modification d'API passe d'abord par ce fichier.**
- [`SOURCES.md`](SOURCES.md) : sources de données vérifiées (URLs, formats, pièges).
- [`SOBRIETE.md`](SOBRIETE.md) : réflexes pour ne pas gaspiller les ressources (mesurer d'abord, où ça compte dans le projet).
- [`EMBARQUE.md`](EMBARQUE.md) : notes perso pour passer au Rust embarqué plus tard.
- [`data/departements.csv`](data/departements.csv) : codes INSEE ↔ noms ↔ codes AlloCiné des départements.

## Répartition du travail

- **Backend (Rust)** : le propriétaire l'écrit **lui-même**. Claude explique, oriente (crates, concepts, pièges) et fait la revue de code, **sans écrire l'implémentation** sauf demande explicite.
- **Frontend** : Claude l'implémente.
- **Doc / CI / déploiement** : Claude, validé par le propriétaire.

## Étapes

### 0. Cadrage ✅
- [x] Analyse de l'ancien code et diagnostic du site en prod
- [x] Choix de la stack et des sources
- [x] Branche `rewrite`, suppression de l'ancien code
- [x] `docs/SOURCES.md`, `docs/API.md`, `docs/PLAN.md`
- [x] Relecture et validation de `docs/API.md` par le propriétaire (validé le 2026-09-28)
- [x] Commit initial de la branche (`11ac7b6`)

### 1. Référentiel des cinémas (backend, propriétaire)
- [x] `cargo new backend`, un binaire avec des sous-commandes `clap` (`import-cinemas`, `scrape`, `serve`), async `tokio`, `anyhow`, `tracing`
  - 2026-09-29 : crate créé, sous-commandes clap OK, compile (`import_cinemas` propage ses erreurs avec `?`). Départements : code INSEE gardé en texte (2A/2B, « 01 »), CSV embarqué avec `include_str!`, lu avec le crate `csv` + serde (struct `Department`). Logs via `tracing` (`EnvFilter` : `RUST_LOG` prioritaire, sinon `info`). Reste : Mayotte à nouveau acceptée avec un code vide (→ `Option<String>`), erreur `csv` à afficher, `info!` récapitulatif, `serve()` sans `unwrap`, imports inutilisés, découpage en modules. Suivi détaillé : [`SUIVI.md`](SUIVI.md)
- [x] Migrations `sqlx` (schéma de `API.md`), `PRAGMA journal_mode=WAL`
  - 2026-09-29 : migrations écrites, WAL + `foreign_keys` + `busy_timeout` dans les options de connexion, pool de 4 connexions (un seul écrivain SQLite, lectures parallèles en WAL). `down` des index corrigé (le préfixe de `DROP INDEX` est une base, pas une table ; pas de `IF EXISTS` pour ne pas masquer les erreurs)
- [x] Scraper les pages AlloCiné par département et la page ville de Paris (pagination, dédoublonnage par ID)
  - 2026-10-02 : parsing hors ligne terminé et testé sur une fixture (`parse_department_page`, `page_count`). Reste : réseau, pagination, dédoublonnage (SUIVI §D)
  - 2026-10-02 : réseau (un seul `Client`, `error_for_status`, contexte d'erreur), pagination, pause de 3 s, dédoublonnage par ID cinéma : OK. Suite : écriture en base, géocodage, CNC, rapport (SUIVI §E-H)
  - 2026-10-03 : source Paris `ville-115755` configurée via `allocine_path`. Deux réimports comparés : 3 121 IDs uniques dans les deux cas, 0 doublon sans 83093 contre 265 avec ; agrégat retiré. Paris : 105 géocodés dans le 75.
- [x] Géocodage en masse par CSV avec l'API Adresse, en loggant les scores faibles
  - 2026-10-03 : premier import complet : 3 024 cinémas, 3 004 géocodés, 146 scores < 0.5. Paris incomplet (10 cinémas) → page `ville-115755` (SUIVI)
- [x] Enrichissement CNC (XLSX via `calamine`) par code INSEE + similarité de nom
  - 2026-10-05 : `enrich-cnc` écrit (à relire) : 1 712 / 1 945 salles fixes croisées (88 %), 1 729 au total, aucun `cnc_id` en double. Lyon et Marseille : pages département AlloCiné incomplètes (7 et 6 cinémas manquants), pages ville ajoutées au CSV le 2026-10-06 (SUIVI §G)
- [x] Rapport : nombre de cinémas, non géocodés, non croisés avec le CNC
  - 2026-10-06 : rapport `info!` en fin d'`import-cinemas` (une requête SQL, testée). Import complet : 3 134 cinémas, 13 sans position, 1 739 croisés CNC, 0 absent.
- [x] Vérification manuelle sur Paris : 107 entrées de la source ville, 105 géocodées dans le 75, aucun ID en double (2026-10-03 ; deux anomalies détaillées dans SUIVI).

### 2. Scraper des séances (backend, propriétaire)
- [ ] Récupération d'une page de séances et désérialisation `serde` (voir `SOURCES.md`)
- [ ] Pagination `p-{n}`
- [ ] Mapping `version` (VF / VO / VOST, film français en VO = VF via la langue du film) et `formats` (liste fermée, cf. `API.md`), vérifié empiriquement sur plusieurs cinémas
- [ ] Upsert des films, remplacement des séances par (cinéma, date) dans une transaction
- [ ] Concurrence + limite de débit globale (`governor` ou sémaphore), retry avec backoff, coupe-circuit sur 403/429
- [ ] Purge des séances passées, table `scrape_runs`

### 3. API (backend, propriétaire)
- [ ] axum : les 8 endpoints de `API.md`, erreurs JSON, gzip (`tower-http`), CORS en dev
- [ ] Recherche insensible aux accents (colonnes `*_search` normalisées)
- [ ] Distance : bounding box en SQL, puis haversine en Rust
- [ ] Tests d'intégration sur une base de test

### 4. Enrichissement TMDB (backend, propriétaire)
- [ ] Clé TMDB (compte gratuit), stockée dans `.env`
- [ ] Croisement par titre original + année, puis bande-annonce, image de fond, note
- [ ] Attribution TMDB affichée dans le front

### 5. Frontend PWA (Claude, en parallèle dès l'étape 0 validée)
- [ ] Vite + Svelte (+ TypeScript), prettier / eslint / vitest
- [ ] Types TS générés à la main depuis `API.md`, données factices (fixtures JSON) tant que l'API n'existe pas
- [ ] Carte MapLibre + OpenFreeMap, regroupement des marqueurs, thème clair/sombre
- [ ] Accueil « à l'affiche » (avec géolocalisation, Paris par défaut)
- [ ] Recherche film / cinéma
- [ ] Fiche film et ses séances par cinéma (filtres VO / heure / date)
- [ ] Fiche cinéma et ses séances
- [ ] État dans l'URL (liens partageables), date calculée à Paris
- [ ] PWA : manifeste, icônes, service worker (cache de la dernière réponse)
- [ ] Responsive mobile d'abord

### 6. Déploiement
- [ ] Réécrire `docs/DEPLOY.md` : binaire + systemd + nginx (front statique, `/api` vers axum)
- [ ] Timer systemd pour les scrapers
- [ ] GitHub Actions : CI (fmt, clippy, tests, build front) + déploiement (binaire + `dist/` par SSH, restart, health check)
- [ ] Migration de la prod : nouvelle base, suppression de l'ancien service Python
- [ ] Merge `rewrite` → `main`

## Idées pour plus tard
- Favoris (cinémas, films) stockés localement
- Filtre « après 20h », formats IMAX / 4DX / Dolby
- Notifications PWA (« tel film sort mercredi »)
- Stratégie de rafraîchissement hors IDF plus fine
