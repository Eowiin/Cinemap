# Cinemap

Carte des cinémas de France et de leurs séances : qu'est-ce qui passe près de moi, où voir tel film, et à quelle heure.

Site : https://cinemap.ethansaux.fr

## Ce que fait le site

- **Une carte** avec plus de 3 000 cinémas en France et les films à l'affiche autour de soi, ou autour d'une ville.
- **Les séances de la semaine** : toute la France, mises à jour chaque nuit. VF ou VO, après une heure donnée, seulement dans ses cinémas favoris.
- **Les cartes d'abonnement** : n'afficher que les cinémas qui acceptent UGC Illimité, Pathé CinéPass, etc.
- **Une fiche par film, par cinéma et par ville** (`/film/…`, `/cinema/…`, `/ville/…`), lisible aussi sans JavaScript.
- **Mon programme** : garder des séances, en enchaîner d'autres à côté, ou laisser Cinemap composer la journée à partir de 1 à 4 films (le moins d'attente possible). Le programme se partage par lien ou s'ajoute à l'agenda.
- **Une app installable** (PWA) qui garde favoris et programme dans le navigateur, sans compte.

## Comment ça marche

Un backend Rust récupère chaque nuit les séances d'AlloCiné : 3 jours, et la semaine entière le mercredi. Il les complète avec TMDB (affiches, synopsis, acteurs), puis les sert au front par une API JSON. Le même binaire sert aussi les pages HTML des films, cinémas et villes, avec leurs métadonnées, ainsi que le sitemap. Les données tiennent dans une base SQLite, sur un petit VPS.

La mesure d'audience se passe de cookie et d'identifiant : le site compte seulement, par jour, les pages vues et les actions (réserver, ajouter une séance…), sans rien garder du visiteur (`cinemap stats` pour la lire). Mentions légales, vie privée et sources : page `/a-propos`.

## Structure

```
backend/    API, pages HTML et scrapers en Rust (axum, sqlx/SQLite, reqwest)
frontend/   PWA Svelte 5 + Vite + MapLibre GL (fond OpenFreeMap)
docs/
  PLAN.md     plan de la réécriture et suivi
  API.md      contrat d'API backend ↔ frontend et schéma SQL
  FRONT.md    choix du frontend
  SOURCES.md  sources de données (AlloCiné, API Adresse, CNC, TMDB)
  CARTES.md   cartes d'abonnement (UGC, Pathé…)
  DEPLOY.md   déploiement sur le VPS (systemd, nginx, CI)
  data/       données de référence (départements, cartes)
```

## Développement

```bash
# API (depuis backend/, base dans backend/database.db)
cargo run -- serve                 # http://127.0.0.1:3000
cargo run -- scrape --department 75
cargo run -- tmdb                  # nécessite TMDB_API_KEY dans backend/.env

# Front (depuis frontend/, Node ≥ 20.19 : `nvm use`)
npm install
npm run dev                        # http://localhost:5173, /api relayé vers :3000
npm test && npm run check && npm run lint
```

Chaque push sur `main` passe la CI (fmt, clippy, tests, build) puis se déploie sur le VPS.

## Plan de la réécriture

Voir [`docs/PLAN.md`](docs/PLAN.md) : objectifs, décisions, étapes et avancement.
