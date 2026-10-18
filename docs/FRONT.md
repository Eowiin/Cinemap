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
| `/programme` | Mon programme, jour par jour (séances enchaînées, composition, trajet), ou un programme partagé (`?date=…&s=…`) |
| `/a-propos` | À propos : mentions légales, vie privée, sources et crédits |

Filtres dans l'URL (liens partageables) : `date` (absente = aujourd'hui, pour qu'un lien reste valable demain), `version` (`VF` / `VO`), `after` (`HH:MM`). La position et le rayon restent dans le navigateur (`localStorage`), pas dans l'URL. « Autour de moi » affiche « Ma position », puis le nom de la ville (géocodage inverse de l'IGN, `data.geopf.fr`, gratuit et sans clé, position arrondie à ~100 m ; hors de France ou en cas d'échec, « Ma position » reste).

Toute page est rechargeable : nginx doit renvoyer `index.html` pour les chemins inconnus (`try_files $uri $uri/ /index.html` dans `deploy/nginx/cinemap.conf`).

## Favoris (2026-10-09)

Cinémas seulement (les films favoris ont été retirés le 2026-10-09, faute d'usage clair ; l'ancienne clé `cinemap:favorites:movies` est effacée au chargement), gardés dans le navigateur (`localStorage`), sans compte ni backend.

- `lib/stored.ts` (pur, testé) + `lib/stored.svelte.ts` : `StoredList<T>`, une liste réactive sous une clé `localStorage`, au format versionné `{ "v": 1, "items": [...] }`. Lecture tolérante (JSON abîmé, autre version, élément invalide ou en double → écarté, jamais d'erreur), stockage indisponible → liste valable pour la session, synchronisée entre onglets (événement `storage`).
- `lib/favorites.ts` / `favorites.svelte.ts` : `cinemap:favorites:cinemas` (`{ id, name, city }`). Chaque favori garde de quoi s'afficher **sans appel à l'API** ; son nom est remis à jour quand on ouvre sa fiche (`refresh`).
- Écran : bouton « Ajouter aux favoris » sur la fiche cinéma ; étoile à côté de chaque cinéma dans les séances d'un film, **mes cinémas affichés en premier** (puis par distance) ; à l'accueil, rangée « Mes cinémas » (lien + retrait) ; filtre « seulement mes cinémas favoris » ; points dorés sur la carte.

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
- **Filtres sur la carte** : favoris et cartes (filtres qui portent sur les cinémas) s'appliquent ; les cinémas écartés restent visibles mais atténués (petits points pâles, non regroupés), pour garder ses repères et ne jamais afficher une carte vide. Jour, version et horaire ne s'appliquent pas (un cinéma sans VO ce soir existe toujours). `filters.ts` → `cinemaMatches`.
- **Bulles** : le nom seul (les cartes acceptées sont sur la fiche du cinéma).
- **Écran tactile** (`(hover: none)`) : un appui montre une fiche (nom, « Voir les séances ») au lieu d'ouvrir le cinéma ; à la souris, survol = nom, clic = fiche cinéma. Noms de tous les cinémas dès le zoom 14.

## Mon programme : mode manuel (2026-10-11)

Nommé « Ma journée » quelques heures (`/journee`), puis **« Mon programme »** (`/programme`) : on prépare n'importe quel jour de la semaine (rangée des 7 jours, avec le nombre de séances prévues), pas seulement aujourd'hui.


- **Un appui sur un horaire** (fiche film ou cinéma) ouvre un menu (`ShowtimeMenu`, un seul pour l'application) : Réserver, Ajouter à mon programme / Retirer, Voir mon programme. Avant l'ajout, il signale ce qui coince avec les séances déjà prévues ce jour-là. Les horaires du programme sont surlignés. Avant, l'horaire menait directement à la réservation.
- **Calculs** (`lib/planner.ts`, pur, testé) : fin = début + 15 min de pubs + `runtime_min` (2 h supposées si inconnue) ; trajet à vol d'oiseau, le plus court entre à pied (4 km/h, 15 min/km) et transports (10 min + 3 min/km), donc à pied sous ~800 m ; battement souhaité 10 min. Statut de chaque enchaînement : chevauchement, trajet trop court, juste (< 10 min de battement), ok. Constantes en tête du fichier, à ajuster avec l'usage.
- **Stockage** : `StoredList` `cinemap:programme`, chaque séance garde de quoi s'afficher et calculer les trajets sans l'API (`PlanStop`). Les jours avant la veille sont effacés au chargement. Icône calendrier dans l'en-tête, avec le nombre de séances prévues.
- **Partage par lien, sans serveur** : `/programme?date=2026-10-12&s=C0159.1000032855.2030,…` (cinéma, film, heure `HHMM` du jour ciné, `2415` = 00h15 ; 8 au plus). Pas l'id de séance : AlloCiné renumérote parfois une séance inchangée (compté par le scrape). À l'ouverture, un appel `/api/cinemas/{id}/showtimes` par cinéma retrouve les séances ; une séance disparue est comptée et signalée. « Ajouter à mon programme » fusionne. Partage natif (`navigator.share`), sinon copie dans le presse-papiers, sinon lien affiché.
- **Agenda** (`lib/ics.ts`, pur, testé) : fichier `cinemap-{date}.ics`, un événement par séance, du début de la séance à la fin estimée, heures en UTC (pas de bloc VTIMEZONE, aucun agenda ne se trompe de fuseau ; testé à l'heure d'été, d'hiver et la nuit du changement), lieu = nom du cinéma + `GEO`, description avec l'heure du film après les pubs et le lien de réservation. Sur iPhone, Safari propose « Tout ajouter ».
- Limite : mon programme garde l'id de séance ; si AlloCiné la renumérote, elle reste affichée dans le programme mais l'horaire n'est plus surligné sur les fiches.
- Vérifié en dev piloté par Chrome sans interface (390 px, clair et sombre) : ajout, alerte de chevauchement, page, lien partagé avec une séance introuvable, aucune erreur console, pas de défilement horizontal.

## « Enchaîner un film » (2026-10-11)

- Revu le soir même : une page « Et après ? » à part, avec une liste à plat (heure · film · cinéma, le même film cinq fois, sans affiche), obligeait à des allers-retours avec le programme. Tout se fait maintenant dans **Mon programme** : sous la dernière séance, « Enchaîner un film » propose les films qu'on peut enchaîner, **un appui sur un horaire l'ajoute**, et la proposition repart de la nouvelle fin.
- **Fermé par défaut** (bouton « + Enchaîner un film » sous la dernière séance) : la plupart des journées n'ont qu'un film, et rien n'est demandé à l'API tant qu'on n'ouvre pas. Refermé après un ajout.
- **Filtre par titre** (au-delà de 5 lignes) : sur les séances déjà chargées, sans requête (`titleMatches` : accents, casse, mots dans le désordre, « spiderman » trouve « Spider-Man »). Film introuvable : « Aucune séance de « X » à attraper après 20:39 à moins de 3 km », avec « Chercher à 5 km » et « Toutes ses séances » (sa fiche). Si le film n'est pas dans les séances chargées (il passe plus tôt, ou ailleurs), un seul `/api/search` après une pause de frappe retrouve sa fiche.
- Une ligne par film et cinéma (affiche, cinéma + trajet, durée, 4 horaires au plus), 5 lignes puis « + n autres ». Horaire « juste » (< 10 min de battement) bordé d'orange.
- `GET /api/showtimes` (`API.md`) autour du cinéma de la dernière séance, `after` = fin estimée, 200 séances au plus, avec les filtres en cours (version, cartes, favoris). Mesuré en local : 99 séances = 33 Ko, **3,8 Ko gzip**.
- Distance : même cinéma (`cinemas=<id>`), 1, 3 (par défaut) ou 5 km. Le filtre favoris ignore la distance, comme ailleurs ; « même cinéma » prime.
- `planner.followUps` / `groupFollowUps` : autre film, ni chevauchement ni trajet trop court, par heure puis distance. Fin après minuit : rien à proposer (`after` ne dépasse pas le jour ciné).
- Menu d'une séance : **Réserver** en grand en tête, puis « Ajouter à mon programme » et « Voir mon programme → ».

## Composer automatiquement (2026-10-11)

- Bloc en haut de **Mon programme**, fermé par défaut. **Deux temps** (revu le soir même : la liste disparaissait dès le premier film choisi) : on **coche** 1 à 4 films dans la liste des films à l'affiche autour (8 puis « Plus de films », recherche par titre, bouton « Composer avec n films » sous la liste), puis les propositions s'affichent (« Modifier » revient à la liste).
- **Composer, c'est recomposer la journée** (revu le même soir : l'automatique complétait autour des séances prévues, et un seul film se collait à la suite) : les films de la journée sont déjà cochés, tous les horaires sont libres, et « Remplacer ce jour » remplace les séances du jour.
- Réglages : « dès » maintenant / 14 h / 18 h… (14 h par défaut un autre jour), 2, 5 ou 10 km autour de la position. Filtres en cours appliqués (version, cartes, favoris). Les films cochés restent dans le navigateur (`cinemap:compose`) : on peut aller voir une fiche et revenir, le bloc se rouvre tel quel.
- **Classement** (`lib/composer.ts`, pur, testé) : le moins d'attente entre les films (trajets compris), un enchaînement « juste » compte 15 min de plus, puis le moins de trajets, puis finir tôt. 3 journées proposées.
- **Calcul** : tous les ordres des films (24 pour 4), et pour chaque film les 6 premières séances possibles après la précédente (une plus tardive n'ajoute que de l'attente). Heures et trajets entre cinémas calculés une fois. Mesuré : 4 films × 50 séances dans 30 cinémas, **~20 ms** (580 ms avant ces caches).
- Un film sans séance dans ces conditions est signalé et la journée se compose sans lui ; si tous ne tiennent pas, les meilleures journées sans l'un d'eux (« Impossible de tout caser ; sans « X » »).
- Requêtes : `/api/movies` pour la liste (souvent en cache), puis un `/api/movies/{id}/showtimes` par film coché, gardé tant que les réglages ne changent pas.

## Trajet de la journée (2026-10-11)

- Sur la carte, les cinémas de Mon programme numérotés dans l'ordre (« 1 · 3 » si on y revient), reliés par une ligne pointillée (`planner.dayRoute`, `map.route`). Rien si tout se passe dans un seul cinéma.
- Mobile : bouton « Voir le trajet sur la carte » (la carte est cachée) ; ordinateur : la carte est déjà là.
- Lien « Itinéraire » sur chaque trajet entre deux séances : Google Maps, à pied ou en transports selon l'estimation. Un lien par trajet : Google ne gère pas les étapes intermédiaires en transports.

## À propos et mentions légales (2026-10-18)

- Texte dans **un seul fichier**, `src/content/about.html` : l'app l'affiche (`AboutPage.svelte`, import `?raw`) et axum l'inclut à la compilation (`backend/src/api/pages/about.rs`, `include_str!`) pour la page lue sans JavaScript. Styles `.about` dans `app.css`.
- Projet personnel : éditeur non professionnel, coordonnées confiées à l'hébergeur (LCEN), hébergeur Hetzner Online GmbH.
- Lien discret en pied du panneau, sur toutes les pages (« Séances mises à jour il y a … · À propos et mentions légales » à l'accueil) ; côté serveur, même lien sous chaque page HTML. Ni bandeau ni pop-up : aucun cookie.
- Page indexable (canonique) mais **hors du sitemap**. Pas d'événement d'audience (`viewEvent` renvoie `null`).

## Couverture (2026-10-09)

Séances mises à jour pour toute la France depuis le 2026-10-12 (avant : l'Île-de-France seulement, avec un message `CoverageNotice` ailleurs, retiré depuis).

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
