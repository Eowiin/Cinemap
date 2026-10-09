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

Contraintes : gratuit (aucune source de données payante), hébergé sur le VPS existant, à **https://cinemap.ethansaux.fr** (décision du 2026-10-08 : `cinemap.eowinstudio.com` est abandonné).

## Décisions

| Sujet | Décision | Pourquoi |
|---|---|---|
| Backend | **Rust** : axum, tokio, sqlx (SQLite), reqwest, scraper, serde, clap, tracing | Le propriétaire apprend Rust ; le backend est **écrit par lui**, Claude guide et relit seulement |
| Frontend | **Svelte + Vite + MapLibre GL** (tuiles OpenFreeMap), PWA via `vite-plugin-pwa` | Léger, réactif, carte vectorielle fluide sur mobile. **Écrit par Claude** |
| Référentiel cinémas | **AlloCiné** (liste par département) + géocodage **API Adresse** + enrichissement **CNC** | Plus aucun rapprochement de noms pour les séances (cause n°1 des bugs de l'ancien site). OSM abandonné |
| Infos films | AlloCiné (déjà dans la réponse des séances) + **TMDB** en complément (bande-annonce, image de fond, note) | TMDB gratuit pour usage non commercial, en français. OMDb écarté (anglais seulement, 1 000 req/jour) |
| Rafraîchissement | Scrap de toute la France chaque nuit, J → J+2 (J+6 le mercredi), concurrent, 1 req/s (`--interval-ms 1000`, limite mesurée le 2026-10-09) | ~2 h à 2 h 30 depuis le VPS. Passage à 7 jours (IDF chaque nuit, hors IDF une fois par semaine) envisagé, voir « Idées pour plus tard » |
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
- [`ETAPE2.md`](ETAPE2.md) : feuille de route détaillée du scraper de séances.
- [`SOBRIETE.md`](SOBRIETE.md) : réflexes pour ne pas gaspiller les ressources (mesurer d'abord, où ça compte dans le projet).
- [`CARTES.md`](CARTES.md) : feuille de route des cartes illimitées (UGC, Pathé…).
- [`EMBARQUE.md`](EMBARQUE.md) : notes perso pour passer au Rust embarqué plus tard.
- [`data/departements.csv`](data/departements.csv) : codes INSEE ↔ noms ↔ codes AlloCiné des départements.

## Répartition du travail

- **Backend (Rust)** : le propriétaire l'écrit **lui-même**. Claude explique, oriente (crates, concepts, pièges) et fait la revue de code, **sans écrire l'implémentation** sauf demande explicite. Le 2026-10-08, le propriétaire a délégué les étapes 3 et 4 à Claude pour avancer : chaque fichier `ETAPE*.md` liste alors les écarts et quelques questions Rust pour s'approprier le code.
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

- [x] Cycle de vie : cinéma masqué de l'API après 14 jours sans être vu, supprimé après 60 jours en fin d'import réussi (`API.md`, 2026-10-06). **Étape 1 terminée le 2026-10-06.**

### 2. Scraper des séances (backend, propriétaire)
Suivi détaillé : [`ETAPE2.md`](ETAPE2.md).
- [x] Récupération d'une page de séances et désérialisation `serde` (voir `SOURCES.md`)
- [x] Pagination `p-{n}`
- [x] Mapping `version` (VF / VO / VOST, film français en VO = VF via la langue du film) et `formats` (liste fermée, cf. `API.md`), vérifié empiriquement sur plusieurs cinémas
- [x] Upsert des films, remplacement des séances par (cinéma, date) dans une transaction
- [x] Concurrence + limite de débit globale (`governor` ou sémaphore), retry avec backoff, coupe-circuit sur 403/429
- [x] Purge des séances passées, table `scrape_runs`
- [ ] Vérification de fin d'étape : run France entière (`caffeinate -i`, `/usr/bin/time -l`), 3 cinémas comparés à la main (voir `ETAPE2.md` §F)

### 3. API (backend, propriétaire)
Suivi détaillé : [`ETAPE3.md`](ETAPE3.md).
- [x] axum : les 8 endpoints de `API.md`, erreurs JSON, gzip (`tower-http`), pas de CORS (proxy Vite en dev)
- [x] Recherche insensible aux accents (colonnes `*_search` normalisées)
- [x] Distance : bounding box en SQL, puis haversine en Rust
- [x] Tests d'intégration sur une base de test (`tests/api.rs`, 19 tests)
- [x] Vérification sur la base de Paris (`curl` des lots D à I, mesures `oha` dans `SOBRIETE.md`) ; pas de re-vérification France entière (décision du 2026-10-08). **Étape 3 terminée le 2026-10-08.**

### 4. Enrichissement TMDB (backend, écrit par Claude à la demande du propriétaire)
Suivi détaillé : [`ETAPE4.md`](ETAPE4.md).
- [x] Clé TMDB (compte gratuit, usage personnel), stockée dans `backend/.env` (`TMDB_API_KEY`)
- [x] Croisement par titre original + année, puis bande-annonce, image de fond, note (`cargo run -- tmdb`, tests sur réponses écrites à la main)
- [x] Attribution TMDB affichée dans le front (fiche film, dès qu'une donnée TMDB est affichée)
- [x] Premier run réel (2026-10-08, Paris) : 312 films croisés sur 344 (91 %), 0 erreur, 77 s ; 12 croisements tirés au hasard tous corrects. **Étape 4 terminée.**

### 5. Frontend PWA (Claude, en parallèle dès l'étape 0 validée)
Suivi détaillé : [`FRONT.md`](FRONT.md).
- [x] Vite + Svelte 5 (+ TypeScript), prettier / eslint / vitest
- [x] Types TS écrits à la main depuis `API.md` (branchés directement sur la vraie API, pas de données factices)
- [x] Carte MapLibre + OpenFreeMap, regroupement des marqueurs, thème clair/sombre
- [x] Accueil « à l'affiche » (avec géolocalisation, Paris par défaut)
- [x] Recherche film / cinéma
- [x] Fiche film et ses séances par cinéma (filtres VO / heure / date)
- [x] Fiche cinéma et ses séances
- [x] État dans l'URL (liens partageables), date calculée à Paris
- [x] PWA : manifeste, icônes, service worker (cache de la dernière réponse)
- [x] Responsive mobile d'abord
- [ ] Essai sur un vrai téléphone (installation PWA, géolocalisation) par le propriétaire

### 6. Déploiement
Suivi détaillé, procédure et écarts : [`DEPLOY.md`](DEPLOY.md). Préparé par Claude le 2026-10-08.
- [x] Réécrire `docs/DEPLOY.md` : binaire + systemd + nginx (front statique, `/api` vers axum), HTTPS (certbot), domaine `cinemap.ethansaux.fr`
- [x] SIGTERM dans `serve` (arrêt propre sous systemd)
- [x] Timers systemd : `scrape` puis `tmdb` chaque nuit à 4 h, `import-cinemas` le mardi à 2 h (`deploy/systemd/`)
- [x] GitHub Actions (`ci.yml`) : CI (fmt, clippy, tests, build front) + déploiement (binaire + `dist/` par SSH, releases versionnées, restart, health check, retour arrière automatique) ; `backend/.sqlx/` versionné, `SQLX_OFFLINE=true`
- [ ] Préparation du VPS par le propriétaire (DNS, ancien service retiré, utilisateur `cinemap`, unités, nginx, certbot) : `DEPLOY.md` étapes 1 à 5
- [ ] Merge `rewrite` → `main` (premier déploiement par la CI)
- [ ] Migration de la prod : base remplie (`cinemap-weekly` puis `cinemap-nightly`), timers activés, ancien service supprimé

## Idées pour plus tard

Tri fait le 2026-10-08 avec le propriétaire, pendant le déploiement de la v1. Ordre de priorité :

1. **Finir le déploiement**, puis **mesurer 2 ou 3 nuits de scrape en prod** (durée, `ralentissements` dans le log) avant de toucher au rafraîchissement.
2. **Cartes illimitées** : UGC Illimité d'abord, puis Pathé CinéPass, puis d'autres cartes si on trouve des listes (pour tous les visiteurs, pas seulement nous). Feuille de route : [`CARTES.md`](CARTES.md). Sources revérifiées le 2026-10-08 : page HTML UGC (145 cinémas avec code postal), JSON Pathé (78 cinémas Pathé avec GPS), PDF Pathé (partenaires sans adresse → CSV à la main).
3. **Favoris** (cinémas, films) stockés dans le navigateur (`localStorage`), sans compte. Front seulement (Claude), peut se faire en parallèle.
4. **Séances sur 7 jours** : aujourd'hui J → J+2 (J+6 le mercredi), car une requête AlloCiné = un cinéma × un jour (~3 100 cinémas : ~9 400 couples par nuit pour 3 jours, dont ~2 900 sautés car annoncés vides). Mesuré le 2026-10-09 (`ETAPE2.md`) : AlloCiné tolère ~1 req/s depuis le VPS, soit ~2 h à 2 h 30 par nuit pour la France sur 3 jours. 7 jours pour toute la France (~5 h) n'est pas raisonnable. Piste :
   - IDF (362 cinémas) : J → J+6 chaque nuit (~1 500 requêtes de plus, ~25 min à 1 req/s) ;
   - hors IDF : J → J+2 chaque nuit, plus la semaine complète la nuit de mardi à mercredi (publication des programmes de la semaine ciné) ;
   - le saut des jours annoncés vides (commit `f39affa`) réduit encore le total.
5. **« Ce soir près de moi »** : prochaines séances de tous les cinémas proches, tous films confondus, triées par heure de début (nouvel endpoint + vue).
6. **Accessibilité** : séances sous-titrées pour sourds et malentendants (`_sme`), audiodescription, salle accessible (tags déjà dans la réponse AlloCiné, à garder en base).

Ensuite, sans ordre :
- Partager une séance (Web Share API) et l'ajouter à son agenda (fichier `.ics`), front seulement.
- Filtres restants : formats IMAX / 4DX / Dolby (déjà dans `formats` côté API, il manque le filtre front et une vérification des vraies valeurs sur une fixture d'un cinéma IMAX), Art et Essai (déjà dans l'API, il manque le filtre front).
- Avant-premières, ciné-rencontres, ressorties de classiques : regarder d'abord ce que contiennent vraiment les tags AlloCiné et `productionYear` en prod.
- Films qui vont bientôt quitter l'affiche (beaucoup moins de séances la semaine suivante) : dépend du point 4.

Fait ou abandonné :
- ~~Filtre « après 20h »~~ : fait (paramètre `after`, `FilterBar.svelte`).
- ~~Notifications PWA~~ : abandonné le 2026-10-08 (serveur Web Push, abonnements côté serveur, iPhone seulement si la PWA est installée : trop lourd pour le gain).
- Stratégie de rafraîchissement hors IDF : intégrée au point 4.
