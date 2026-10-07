# Sources de données

Tout ce qu'on sait des sources, vérifié le 2026-09-25. À relire avant d'écrire les scrapers.

## 1. AlloCiné — liste des cinémas (référentiel)

Page HTML par département, paginée :

```
GET https://www.allocine.fr/salle/cinema/{allocine_path}/
GET https://www.allocine.fr/salle/cinema/{allocine_path}/?page=2
```

- Les chemins des pages AlloCiné sont dans [`data/departements.csv`](data/departements.csv) (colonnes `code_insee,nom,allocine_path`) : `departement-…` pour les départements, `ville-115755` pour Paris. Mayotte n'a pas de code connu. AlloCiné référence pourtant au moins une salle à Mayotte (`W9762`, Salle de Chirongui, vu le 2026-09-30) : à rattacher plus tard, par exemple en ajoutant son ID à la main.
- ~~Paris n'a pas de page propre : on utilise `83093`~~ **Corrigé le 2026-10-03** : la page `departement-83093` est bien une page agrégée Île-de-France (15 pages × 50), mais elle ne contient qu'**une dizaine de cinémas parisiens** (page 1 seulement, les grands multiplexes) ; le premier import complet n'a trouvé que 10 cinémas dans le 75. Paris a une page **ville** : `https://www.allocine.fr/salle/cinema/ville-115755/` (« Cinéma à Paris (75000) »), **20 cinémas par page**, 6 pages (5 × 20 + 7 = 107 le 2026-10-03), même structure HTML (`data-theater`, `<address>`), pagination `?page=N`. → Pour Paris, utiliser `ville-115755` à la place de 83093.
- **Lyon et Marseille (2026-10-06)** : même problème, en moins grave. La page département n'a pas tous les cinémas de la ville : Lyon `ville-113315` (17 cinémas, 7 absents de `departement-83196`), Marseille `ville-87914` (16, 6 absents de `departement-83188`). Ici, il faut **les deux** pages (le reste du département n'est que sur la page département) → deux lignes avec le même `code_insee` dans le CSV. Les autres pages ville testées (Aix, Arles, La Ciotat, Martigues, Salon, Vitrolles, Vaulx-en-Velin, Villefranche) n'apportent rien : seules les villes à arrondissements semblent concernées. Les liens `ville-…` se trouvent sur la page 1 du département.
- Comparaison réelle le 2026-10-03 après correction : **3 121 IDs uniques** avec ou sans `83093`, **265 doublons avec contre 0 sans**. L’agrégat n’apporte aucun cinéma supplémentaire ; il est retiré du CSV. Paris ville fournit 107 entrées, dont 105 géocodées dans le 75 (deux anomalies détaillées dans `SUIVI.md`).
- Les pages répondent `301` sans `/` final (`departement-83093?page=2` → `departement-83093/?page=2`) ; reqwest suit la redirection, mais autant écrire directement l'URL avec `/`.
- Le nombre de pages se lit dans les liens `?page=N` de la pagination.

Dans le HTML, chaque cinéma a :

- un attribut `data-theater` contenant du JSON : `{"id":"C0159","name":"UGC Ciné Cité Les Halles"}` (entités HTML à décoder, `é` à désérialiser) ;
- une balise `<address class="address …">7 Place de la Rotonde 75001 Paris</address>` dans la même carte (`.theater-card`).

L'ID (`C0159`, `W7520`, `B0045`…) est l'identifiant utilisé partout ensuite → **c'est notre clé primaire cinéma**.

## 2. API Adresse (géocodage) — gratuite, sans clé

- Unitaire : `GET https://api-adresse.data.gouv.fr/search/?q=7+Place+de+la+Rotonde+75001+Paris&limit=1`
- En masse (recommandé, une seule requête pour ~2 000 adresses) : `POST https://api-adresse.data.gouv.fr/search/csv/` en multipart avec un CSV (`data=@fichier.csv`, `columns=adresse`). Réponse = le CSV enrichi de `latitude`, `longitude`, `result_score`, `result_citycode` (code INSEE), `result_postcode`, `result_city`.
- Vérifié le 2026-10-02 : l'ancien domaine répond encore, mais le service officiel est la Géoplateforme IGN : `https://data.geopf.fr/geocodage/search` et `…/geocodage/search/csv` (mêmes paramètres, même CSV : `latitude`, `longitude`, `result_score`, `result_postcode`, `result_city`, `result_citycode`, `result_status`…).
- Garder `result_score` : sous ~0.5, le résultat est douteux → le logger.
- `result_citycode` donne le code INSEE commune → sert à croiser avec le CNC et à déduire le département.

## 3. CNC — établissements actifs (enrichissement)

- XLSX : `https://www.data.gouv.fr/api/1/datasets/r/cdb918e7-7f1a-44fc-bf6f-c59d1614ed6d`
- Une feuille par année (`2025`, `2024`, …) → prendre la plus récente. En-têtes en **ligne 5**, données à partir de la ligne 6, ~2 060 lignes.
- Colonnes : `NAutoC` (n° d'autorisation), `NomEtab` (MAJUSCULES, parfois avec espaces parasites), `Ecrans`, `fauteuils`, `DEPCOM` (code INSEE commune), `COMMUNE`, `genre` (`FIXE` / itinérant…), `AE` (`OUI`/`NON`), `3D`.
- Pas de GPS, pas d'adresse. Sert uniquement à ajouter `screens`, `seats`, `art_et_essai`.
- Croisement : même `DEPCOM` que le `result_citycode` du géocodage **puis** similarité de nom. Un échec de croisement n'est pas grave (le cinéma perd juste ses infos CNC).
- Paris : `DEPCOM` est par arrondissement (`75101`…`75120`), comme l'API Adresse → bon match.
- **Lyon et Marseille** : au contraire, `DEPCOM` est la commune entière (`69123`, `13055`), alors que l'API Adresse renvoie l'arrondissement (`69381`…, `13201`…) → ramener ces arrondissements à la commune avant de comparer (vérifié le 2026-10-05).

## 4. AlloCiné — séances (API interne non officielle)

```
GET https://www.allocine.fr/_/showtimes/theater-{id}/d-{YYYY-MM-DD}/
GET https://www.allocine.fr/_/showtimes/theater-{id}/d-{YYYY-MM-DD}/p-{page}/
```

Headers utilisés : `User-Agent` navigateur, `Accept: application/json`, `Referer: https://www.allocine.fr/`.

### ⚠️ Pagination

La réponse est **paginée par film** : 15 films par page.
```json
"pagination": {"page": 1, "totalPages": 3, "itemsPerPage": 15, "totalItems": 32}
```
L'ancien scraper ne lisait que la page 1 → il manquait plus de la moitié des films des gros cinémas.

### Structure

```text
{
  error: bool, message, pagination, facets, …
  results: [
    {
      movie: {
        internalId: 1000032855,           // ID film AlloCiné → notre clé primaire film
        title, originalTitle,
        runtime: "1h 52min",             // parfois "0h 00min" = inconnu
        genres: [{translate: "Epouvante-horreur", tag: "HORROR"}],
        synopsis (texte), synopsisFull (HTML),
        poster: {url},
        credits: [{person: {firstName, lastName}, position: {name: "DIRECTOR"}}],
        cast: {edges: [{node: {actor: {firstName, lastName}, role}}]},
        releases: [{releaseDate: {date: "2026-09-30"}, certificate: {label: "Interdit - 16 ans"}}],
        countries: [{localizedName}],
        data: {productionYear},
        stats: {userRating, pressReview, …},
        languages
      },
      showtimes: {
        // clés : original, original_st, original_sme, multiple, multiple_st, multiple_sme, local, dubbed, …
        "<clé>": [
          {
            internalId,                  // ID séance
            startsAt: "2026-09-25T20:00:00",  // heure locale Paris, SANS fuseau
            diffusionVersion: "ORIGINAL" | "DUBBED" | "LOCAL" | …,
            experience: null | [...],    // IMAX, 4DX, ScreenX, Dolby…
            projection: ["DIGITAL"] | ["3D"],
            tags: ["Localization.Version.French", …],
            data: {ticketing: [{urls: ["https://…"]}]}   // lien de réservation !
          }
        ]
      }
    }
  ]
}
```

Les clés `*_st` indiquent des sous-titres (VOST). À vérifier empiriquement sur plusieurs cinémas avant de figer le mapping vers `VO` / `VOST` / `VF`.

### Mapping de version vérifié sur les fixtures (2026-10-06)

| Groupe | `diffusionVersion` | Tags `Localization.*` | Langues du film | Version retenue |
|---|---|---|---|---|
| `multiple` | `DUBBED` | `Version.French` | `FRENCH` | `VF` (pour un film français, `DUBBED` seul ne signifie pas doublage) |
| `multiple` | `DUBBED` | `Version.French` + `Subtitle.French` | `FRENCH` | `VF` (sous-titres français liés à l'accessibilité) |
| `original` | `ORIGINAL` | `Version.Original` + `Subtitle.French` | `ENGLISH`, `JAPANESE`… | `VOST` |
| `original` | `ORIGINAL` | `Version.Original` | `ENGLISH`… | `VO` |
| `original` | `ORIGINAL` | `Version.Original` | `FRENCH` | `VF` (film français en version originale) |
| `original` | `ORIGINAL` | `Version.Original` | `CANTONESE`, `FRENCH` | `VO` selon la règle actuelle : seule la première langue est prise en compte |
| `multiple_sme` | `LOCAL` | `Language.French` (+ `Accessibility.Subtitled`) | `FRENCH` | `VF` (vu sur P0095 le 2026-10-08 : film français avec sous-titres SME) |
| `original_st` | `ORIGINAL` | `Version.Original` + `Accessibility.Subtitled` (pas de `Subtitle.French`) | `TURKISH` | `VOST` (vu sur P0095 le 2026-10-08) |

Les formats persistés sont limités à `3D`, `IMAX`, `4DX`, `ScreenX`, `Dolby Cinema` et `Dolby Atmos`. Les valeurs `DIGITAL`, `ANALOG` et `DOLBY_71` observées dans les fixtures ne sont pas des formats retenus.

### Accessibilité observée

Tags relevés : `Showtime.Accessibility.Accessible` et `Theater.Service.DisabledAccess`. Certaines URLs relay contiennent également `SME`. Ces informations sont notées pour référence et ne sont pas enregistrées en base pour l'instant.

### Bonnes pratiques

- Débit global limité (~3 req/s), quelques requêtes concurrentes max, retry avec backoff exponentiel.
- Coupe-circuit si trop de 403/429 d'affilée (on s'est peut-être fait bloquer).
- Une séance absente de la nouvelle réponse = annulée → remplacer toutes les séances d'un couple (cinéma, date) dans une transaction.

## 5. TMDB — enrichissement des films (optionnel)

AlloCiné fournit déjà synopsis, casting, genres, affiche. TMDB apporte en plus : bande-annonce, image de fond, durée fiable, note.

- Gratuit pour usage non commercial, clé via un compte TMDB. Attribution obligatoire (logo + « This product uses the TMDB API but is not endorsed or certified by TMDB »).
- Limite ~50 req/s, pas de quota journalier.
- `GET https://api.themoviedb.org/3/search/movie?query=…&year=…&language=fr-FR`, puis `GET /3/movie/{id}?append_to_response=videos,credits&language=fr-FR`.
- Croisement AlloCiné → TMDB par `originalTitle` + `productionYear` (fallback `title`).
