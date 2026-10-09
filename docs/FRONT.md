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

## Favoris (2026-10-09)

Cinémas et films, gardés dans le navigateur (`localStorage`), sans compte ni backend.

- `lib/stored.ts` (pur, testé) + `lib/stored.svelte.ts` : `StoredList<T>`, une liste réactive sous une clé `localStorage`, au format versionné `{ "v": 1, "items": [...] }`. Lecture tolérante (JSON abîmé, autre version, élément invalide ou en double → écarté, jamais d'erreur), stockage indisponible → liste valable pour la session, synchronisée entre onglets (événement `storage`).
- `lib/favorites.ts` / `favorites.svelte.ts` : `cinemap:favorites:cinemas` (`{ id, name, city }`) et `cinemap:favorites:movies` (`{ id, title, poster_url }`). Chaque favori garde de quoi s'afficher **sans appel à l'API** ; son nom est remis à jour quand on ouvre sa fiche (`refresh`).
- Écran : bouton « Ajouter aux favoris » sur les fiches cinéma et film ; étoile à côté de chaque cinéma dans les séances d'un film, **mes cinémas affichés en premier** (puis par distance) ; à l'accueil, rangées « mes cinémas » et « mes films » (lien + retrait), étoile sur les films favoris à l'affiche.

## Cartes d'abonnement (2026-10-09)

- Revu le 2026-10-09 (le système « mes cartes » + bouton « Ma carte » était déroutant) : **un bouton par carte dans `FilterBar`**, un clic filtre tout de suite (paramètre `cards` de l'URL, lien partageable, envoyé à `/api/movies` et `/api/movies/{id}/showtimes`).
- Le dernier filtre est retenu (`StoredList` `cinemap:cards`, `lib/cards.svelte.ts`) et réappliqué à l'ouverture si l'URL n'a pas de `cards` (un lien partagé décide).
- Badges (`CardBadges.svelte`) : toutes les cartes acceptées, celles du filtre en couleur ; fiche cinéma (avec « d'après les listes … du 09/10 »), séances d'un film, popup de la carte.

## Filtres (revus le 2026-10-09)

- Rangée des jours toujours visible : la semaine (J à J+6), les jours sans séances grisés au lieu d'être cachés (un jour au-delà vient d'un lien partagé). `lib/filters.ts` (pur, testé).
- Bouton « Filtres (n) » → panneau `<dialog>` (feuille en bas sur mobile, fenêtre centrée dès 640 px) : Version, Horaire, Cinémas (« seulement mes cinémas favoris »), Carte d'abonnement. Les changements s'appliquent tout de suite ; « Tout effacer » / « Voir les séances ».
- Filtres actifs en pastilles retirables à côté du bouton.
- Favoris : `favoris=1` dans l'URL ; les ids viennent du navigateur et partent en `cinemas=` (`API.md`), **sans rayon**. Sur la fiche d'un cinéma (`scope="cinema"`), seuls Version et Horaire.
- Vérifié en build de prod piloté par Chrome sans interface (captures mobile 390 px et bureau 1280 px, mode sombre, pas de défilement horizontal).

## Carte (2026-10-09)

- **Cadrage retenu** : le dernier cadrage libre de l'accueil (`map.camera`, dans `app.svelte.ts` car la carte est détruite quand on la ferme sur mobile) est restauré au retour d'un cinéma ou d'un film, tant que la position et le rayon n'ont pas changé.
- **Favoris** : calque à part, non regroupé, doré, avec leur nom dès le zoom 10.
- **Écran tactile** (`(hover: none)`) : un appui montre une fiche (nom, ville, cartes, « Voir les séances ») au lieu d'ouvrir le cinéma ; à la souris, survol = nom, clic = fiche cinéma. Noms de tous les cinémas dès le zoom 14.

## Couverture (2026-10-09)

Séances mises à jour pour l'Île-de-France seulement (`lib/coverage.ts`). Message `CoverageNotice` : accueil et fiche film si la position est hors IDF, fiche d'un cinéma hors IDF.

## Carte indisponible

`MapView` affiche un message et le détail technique (au lieu d'un fond gris) si WebGL 2 manque, si MapLibre ne se charge pas ou si le fond de carte ne répond pas. Signalé : fond sombre avec les boutons +/− et aucun point sur un Android (Chrome) : le style OpenFreeMap ne se charge probablement pas (bloqueur, « DNS privé »). Garde-fou : message au bout de 20 s sans style chargé ; le détail inclut le GPU et le user agent.

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
