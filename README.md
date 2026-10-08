# Cinemap

Carte des cinémas de France et de leurs séances : qu'est-ce qui passe près de moi, où voir tel film, et à quelle heure.

Site : https://cinemap.eowinstudio.com

## Structure

```
backend/    API + scrapers en Rust (axum, sqlx/SQLite, reqwest)
frontend/   PWA Svelte + Vite + MapLibre GL
docs/
  PLAN.md     plan de la réécriture et suivi
  API.md      contrat d'API backend ↔ frontend et schéma SQL
  SOURCES.md  sources de données (AlloCiné, API Adresse, CNC, TMDB)
  DEPLOY.md   déploiement sur le VPS (à réécrire pour la nouvelle stack)
  data/       données de référence (codes départements)
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

## Plan de la réécriture

Voir [`docs/PLAN.md`](docs/PLAN.md) : objectifs, décisions, étapes et avancement.
