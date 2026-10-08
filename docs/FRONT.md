# Frontend (étape 5)

Écrit par Claude le 2026-10-08. Svelte 5 (runes) + Vite 8 + TypeScript, MapLibre GL 6 + OpenFreeMap, PWA via `vite-plugin-pwa`. Pas de SvelteKit ni de bibliothèque de routage : un routeur de 60 lignes suffit pour 3 pages.

## Lancer

```bash
cd frontend
nvm use                 # Node 24 (.nvmrc) ; Vite 8 demande Node ≥ 20.19
npm install
npm run dev             # http://localhost:5173, /api relayé vers http://localhost:3000 (cargo run -- serve)
npm test                # vitest : fonctions pures (dates à Paris, filtres, URL)
npm run check           # svelte-check (types)
npm run lint            # eslint + prettier
npm run build           # dist/ (statique, servi par nginx en prod)
```

## Pages et URL

| URL | Page |
|---|---|
| `/` | À l'affiche autour de la position (Paris par défaut, bouton « Autour de moi »), rayon 5 à 100 km |
| `/film/{id}` | Fiche film + séances par cinéma, triées par distance |
| `/cinema/{id}` | Fiche cinéma + programme du jour |

Filtres dans l'URL (liens partageables) : `date` (absente = aujourd'hui, pour qu'un lien reste valable demain), `version` (`VF` / `VO`), `after` (`HH:MM`). La position et le rayon restent dans le navigateur (`localStorage`), pas dans l'URL.

Toute page est rechargeable : nginx doit renvoyer `index.html` pour les chemins inconnus (`try_files $uri $uri/ /index.html` dans `deploy/nginx/cinemap.conf`).

## Les bugs de l'ancien site, et ce qui les évite

- **Date en UTC** : `parisNow()` passe par `Intl` avec `Europe/Paris` ; le jour de référence vient de `/api/meta` (`today`). Testé autour de minuit et du passage à l'heure d'hiver.
- **Filtre non recalculé au changement de date** : les filtres envoyés à l'API sont dérivés de l'URL (`showtimeFilters()`), recalculés à chaque changement.
- **Réponses dans le désordre** : `resource()` annule la requête précédente (`AbortController`) ; une vieille réponse ne peut plus écraser la nouvelle.
- **Plantage hors filtre** : une liste vide est un état normal (message + « chercher plus loin »), jamais une erreur.

## Sobriété

- Code de l'application : **31 Ko gzip**. MapLibre (**289 Ko gzip** + son worker) est chargé **à part et seulement quand la carte s'affiche** : sur mobile, rien n'est téléchargé tant qu'on n'ouvre pas la carte. Idem pour la liste des 3 100 cinémas (98 Ko gzip).
- Le service worker ne précharge que l'application (**102 Ko**) ; MapLibre, les tuiles et les affiches sont mis en cache au premier usage.
- Affiches redimensionnées par le CDN AlloCiné (`c_{l}_{h}/`) : ~11 Ko au lieu de ~270 Ko par vignette.
- « Maintenant » est arrondi au quart d'heure : la requête change 4 fois par heure (pas chaque minute), ce qui laisse jouer le cache HTTP de 5 min, et une séance commencée il y a 10 min (pubs) reste affichée.
- Recherche : une requête par pause de frappe (200 ms), pas une par lettre.

## Vérifié

Avec l'API sur la base de Paris, en build de production (`vite preview`) piloté par Chrome (protocole DevTools) : accueil, fiche film, fiche cinéma, recherche (« montreuil » → le Méliès), carte mobile, mode sombre, aucun défilement horizontal à 390 px, aucune erreur console.

Pas encore vérifié : sur un vrai téléphone (installation PWA, géolocalisation, hors-ligne).

## Pistes

- Cercle du rayon sur la carte, bouton « chercher dans cette zone ».
- Lecteur YouTube intégré (aujourd'hui : lien vers YouTube).
- Bandeau « hors-ligne, données d'il y a X » quand le service worker sert le cache.
