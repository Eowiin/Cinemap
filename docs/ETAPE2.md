# Étape 2 : scraper des séances (feuille de route détaillée)

Même principe que `SUIVI.md` pour l'étape 1 : le **quoi** et le **pourquoi**, avec des indices ; le **comment**, c'est toi.
Chaque lot se termine par une vérification chiffrée. Références : `SOURCES.md` §4 (API AlloCiné), `API.md` (schéma `movies` / `showtimes`, conventions `version` / `formats` / jour ciné).

Dernière mise à jour : 2026-10-06. **Prochain lot : A.**

## Vue d'ensemble

```text
pour chaque cinéma visible × chaque jour ciné (J → J+2, J+6 le mercredi)
  └─ GET /_/showtimes/theater-{id}/d-{date}/ (+ /p-{n}/ tant que page < totalPages)
       └─ JSON → structs serde → films + séances (version, formats, lien de réservation)
            └─ une transaction : upsert des films, remplacement des séances (cinéma, date)
fin : purge des séances passées, ligne dans scrape_runs, rapport
```

Ordre conseillé : tout ce qui se teste **hors ligne** d'abord (A, B), puis **un** cinéma en réseau (C), la base (D), et seulement ensuite toute la France (E, F).

Ce qu'on sait déjà d'une vraie réponse (C0159, 2026-10-06, vérifiée par Claude) :

- `pagination` : `{"page": 1, "totalPages": 2, "itemsPerPage": 15, "totalItems": 30}`.
- `results[]` : `{ movie, showtimes }`. `showtimes` est un objet dont les **clés** sont des groupes : `original`, `original_st`, `original_sme`, `multiple`, `multiple_st`, `multiple_sme`, `local`, `dubbed`… (beaucoup de groupes vides).
- Un film français (`languages: ["FRENCH"]`) arrive dans `multiple` avec `diffusionVersion: "DUBBED"` et le tag `Localization.Version.French` : le mot `DUBBED` ne veut donc **pas** dire « doublé » ici. D'où l'exploration du lot B avant de figer le mapping.
- Un film étranger en VOST : groupe `original`, `ORIGINAL`, tags `Localization.Version.Original` + `Localization.Subtitle.French`.
- `_sme` = sous-titres pour sourds et malentendants (à confirmer). On voit aussi des tags `Showtime.Accessibility.Accessible`, `Theater.Service.DisabledAccess`.
- `data.ticketing` : **plusieurs** liens (`provider` : `relay`, celui du cinéma…, `type` : `DESKTOP` / `MOBILE`). Il faudra choisir.
- `runtime` : `"1h 55min"`.

---

## A. Fixtures et désérialisation (hors ligne)

1. **Fixtures** : télécharge à la main avec `curl` (trois `-H` : `User-Agent`, `Accept: application/json`, `Referer`), pour aujourd'hui :
   - `C0159` pages 1 **et** 2 (gros multiplexe, VF + VOST) ;
   - un cinéma Art et Essai parisien (beaucoup de VOST, peut-être des ressorties) ;
   - un petit cinéma de province (une seule page, peu de séances) ;
   - un cinéma **un jour sans séance** (regarde ce que deviennent `results` et `nextDate`).

   Range-les dans `tests/fixtures/showtimes-<id>-<date>-p<n>.json`. Explore-les avec `jq` (`jq '.results[0].movie | keys'`, `jq '.results[].showtimes | keys'`).
2. **Module** `src/showtimes/` avec `mod.rs` (orchestration) et `allocine.rs` (types + parsing), comme `cinemas/`.
3. **Structs serde** : `Response { error, pagination, results }`, `Result { movie, showtimes }`, `Movie { … }`, `Showtime { … }`. Seulement les champs utiles (serde ignore le reste par défaut). Pistes :
   - `#[serde(rename_all = "camelCase")]` sur la struct évite un `rename` par champ (`internalId`, `startsAt`, `diffusionVersion`).
   - Un champ qui peut être `null` **ou absent** : `Option<T>` + `#[serde(default)]`.
   - Les **clés variables** de `showtimes` : quelle collection de `std::collections` serde sait remplir depuis un objet JSON aux clés inconnues ? Et si tu veux un ordre stable (pour les tests et les logs) ?
   - Les objets imbriqués (`poster.url`, `releases[0].releaseDate.date`, `credits[].person`) : soit des petites structs, soit `serde_json::Value` pour explorer. Préfère les structs une fois la forme connue.
   - `internalId` de séance est un **grand nombre** (`82306178985`) : quel type entier ? Le schéma le stocke en `TEXT`.
4. **Fonctions pures** (dans `showtimes/` ou `text.rs`), chacune avec ses tests :
   - `runtime_minutes("1h 55min") -> Option<u32>` ; `"0h 00min"` → `None` (inconnu, pas 0) ; une chaîne bizarre → `None`, pas de panique.
   - `full_name(first, last)` pour réalisateurs et acteurs (l'un des deux peut manquer).
5. **Tests** :
   - `C0159` p1 : 15 films, `totalPages == 2`, `totalItems == 30` ; un film précis a son `internalId`, son titre et au moins une séance avec `startsAt` et un lien.
   - Jour sans séance : la désérialisation **réussit** et donne 0 film.

**Vérifier** : `cargo test` vert ; `cargo clippy --all-targets` sans warning.

## B. Mapping version, formats et lien (fonctions pures)

1. **Exploration** : un test (`#[ignore]`, lancé avec `cargo test -- --ignored --nocapture`) ou un `debug!` qui parcourt **toutes** les fixtures et affiche les combinaisons distinctes : (clé du groupe, `diffusionVersion`, tags `Localization.*`, `movie.languages`). Note le tableau dans `SOURCES.md` §4.
2. **Décide** le mapping à partir du tableau, puis écris `fn version(group: &str, showtime: &…, movie_languages: &[…]) -> Version` avec `enum Version { Vf, Vo, Vost }`. Rappels de `API.md` : film français en version originale = `VF` ; `VO` = sans sous-titres ; `VOST` = sous-titré. Question : que fais-tu d'un groupe inconnu ? (Indice : `warn!` + on saute la séance, plutôt que deviner.)
3. **Formats** : liste fermée de `API.md` (`3D`, `IMAX`, `4DX`, `ScreenX`, `Dolby Cinema`, `Dolby Atmos`). Regarde où chaque info arrive dans les fixtures (`experience`, `projection`, `sound`, `picture`, `tags`). Toute autre valeur : ignorée mais **loggée une seule fois** (un `HashSet` des valeurs déjà vues). Sérialisation en base : `serde_json::to_string(&vec)`.
4. **Lien de réservation** : plusieurs `ticketing`. Règle à choisir et à écrire en commentaire, par exemple « `DESKTOP` du fournisseur du cinéma s'il existe, sinon `relay`, sinon `None` ». Regarde les URLs des fixtures pour trancher.
5. **Accessibilité** (pour plus tard, ne pas le stocker encore) : note dans `SOURCES.md` les tags vus (`_sme`, `Accessible`, audiodescription ?). Ça pourra devenir un filtre (voir les idées dans `PLAN.md`).

**Vérifier** : tests de `version` sur un cas de chaque ligne du tableau (au moins : film français, VOST, VF doublée, VO sans sous-titres si tu en trouves) ; tests de `formats` (IMAX, 3D, valeur inconnue ignorée).

## C. Un cinéma, une date, en réseau

1. **Requête** : réutilise `client::fetch_with_retries`. La closure ajoute `Accept: application/json` et le `Referer` (regarde `allocine_request` dans `cinemas/allocine.rs` : faut-il la rendre publique ou en écrire une variante ?).
2. **URL** : `…/_/showtimes/theater-{id}/d-{date}/` puis `…/p-{n}/` (avec le `/` final, cf. le 301 évité pour les listes de cinémas). Fonction pure `showtimes_url(id, date, page)` + test.
3. **Pagination** : lire `totalPages` dans la page 1, puis les suivantes. Pause entre deux pages (pour l'instant fixe, comme dans `get_cinemas_from_department`).
4. **Erreurs** : `error: true` dans un JSON 200 → erreur avec `message`. 403 / 429 → **pas** de nouvel essai (déjà le cas dans `is_retryable`) : on les traitera au lot E.
5. **Sous-commande de test** : `scrape --cinema C0159 --date 2026-10-07` (`clap` : `Option<String>` + `#[arg(long)]`) qui affiche le nombre de films et de séances, sans écrire en base.

**Vérifier** : `cargo run -- scrape --cinema C0159` → 30 films (ou ce qu'affiche le site AlloCiné pour ce jour), nombre de séances cohérent avec la page web.

## D. Écriture en base

1. **Jour ciné** : la date demandée à AlloCiné, pas la date de `starts_at` (`API.md`, Conventions). Et « aujourd'hui » = aujourd'hui **à Paris**, pas en UTC (bug n°1 de l'ancien front). Crate pour le fuseau : `jiff` (`Zoned::now().in_tz("Europe/Paris")`) ou `chrono` + `chrono-tz`. Fonction `cine_dates(today, is_wednesday…) -> Vec<Date>` pure et testée (J → J+2, J+6 le mercredi : vérifie la règle dans `PLAN.md`).
2. **Films** : upsert (`ON CONFLICT(id) DO UPDATE`), colonnes JSON (`genres`, `directors`, `cast_members`, `countries`) via `serde_json::to_string`. `title_search` = `normalize(title)`. Ne pas écraser les colonnes TMDB (`tmdb_id`, `rating`…) : elles ne sont pas dans la requête.
3. **Séances** : pour un couple (cinéma, date), `DELETE` puis `INSERT` dans **une** transaction (une séance disparue = annulée, `SOURCES.md`). Les films d'abord (clé étrangère `movie_id`).
4. **Transaction courte** : tout le réseau d'un (cinéma, date) est fini **avant** `pool.begin()` (même règle qu'au géocodage, `SOBRIETE.md`).
5. **Tests** sur base en mémoire (modèle : `save_matches_replaces_previous_cnc_data` dans `cnc.rs`) : un second passage sans une séance la supprime ; les séances d'une **autre** date du même cinéma restent ; un film déjà enrichi par TMDB garde son `tmdb_id`.

**Vérifier** : après `scrape --cinema C0159` (cette fois avec écriture) :
```sql
select count(*), count(distinct movie_id) from showtimes where cinema_id = 'C0159' and date = '…';
select version, count(*) from showtimes group by 1;
select count(*) from showtimes where booking_url is null;
```

## E. Toute la France : concurrence, débit, coupe-circuit

1. **Liste des cibles** : cinémas **visibles** seulement (`lat IS NOT NULL` et vus depuis moins de 14 jours, `API.md` « Cycle de vie »). Combien de requêtes ? Fais le calcul : ~3 100 cinémas × 3 dates × (pages par cinéma, en moyenne ?) ; à 3 req/s, combien de minutes ? Compare aux « ~30-40 min » de `PLAN.md` : faut-il revoir le débit, les dates, ou scraper l'IDF plus souvent que le reste ?
2. **Concurrence** : `futures::stream::iter(cibles).map(|c| async { … }).buffer_unordered(n)` (crate `futures`). `n` petit (3-4).
3. **Débit global** (toutes tâches confondues) : crate `governor` (un `RateLimiter` partagé, `until_ready().await` avant chaque requête), ou un `tokio::time::interval` derrière un `Mutex`. Question : pourquoi un sémaphore seul ne limite-t-il **pas** le débit ?
4. **Coupe-circuit** : compteur de 403/429 **consécutifs** partagé (`Arc<AtomicUsize>`, remis à 0 à chaque succès). Au-delà de N (5 ?), on arrête tout proprement : plus aucune requête, on garde ce qui est déjà écrit, rapport d'erreur.
5. **Erreurs isolées** : un cinéma en erreur ne doit pas arrêter le run (contrairement à l'import des cinémas). On compte ok / erreurs.
6. **Écritures** : SQLite n'a qu'un écrivain. Soit chaque tâche écrit sa transaction courte (le `busy_timeout` fait attendre les autres), soit une seule tâche écrivain qui reçoit les résultats par `tokio::sync::mpsc`. Mesure avant de compliquer.

**Vérifier** : d'abord sur un **département** (option `--department 75`), puis toute la France lancée par toi avec `caffeinate -i` et `/usr/bin/time -l` (durée, mémoire max, CPU → `SOBRIETE.md`).

## F. Purge, suivi des runs, rapport

1. **Purge** des séances passées (`date < aujourd'hui Paris`) en fin de run. Films sans aucune séance : les garder (TMDB les a peut-être enrichis), ou les purger après N jours ? À décider.
2. **`scrape_runs`** : une ligne au début (`kind = 'showtimes'`, `started_at`), mise à jour à la fin (`finished_at`, `ok_count`, `error_count`). Un run interrompu garde `finished_at = NULL` : utile pour `/api/meta` (date de dernière mise à jour).
3. **Rapport** (`info!`, sur le modèle d'`import_report`) : cinémas scrapés / en erreur, films, séances par version, séances sans lien, durée.

**Vérifier (fin de l'étape 2, à cocher dans `PLAN.md`)** :
- 3 cinémas comparés à la main avec le site AlloCiné (un multiplexe, un Art et Essai, un petit) : mêmes films, même nombre de séances, versions justes.
- Aucun film manquant sur les gros cinémas (pagination) : `select cinema_id, date, count(distinct movie_id) from showtimes group by 1, 2 order by 3 desc limit 5`.
- Durée, mémoire et nombre de requêtes d'un run complet notés dans `SOBRIETE.md`.
