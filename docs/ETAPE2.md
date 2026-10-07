# Étape 2 : scraper des séances (feuille de route détaillée)

Même principe que `SUIVI.md` pour l'étape 1 : le **quoi** et le **pourquoi**, avec des indices ; le **comment**, c'est toi.
Chaque lot est découpé en petites étapes : **fichier à toucher → quoi y mettre → comment vérifier**. Une case = un petit pas qu'on peut compiler.
Références : `SOURCES.md` §4 (API AlloCiné), `API.md` (schéma `movies` / `showtimes`, conventions `version` / `formats` / jour ciné).

Dernière mise à jour : 2026-10-08. **Où tu en es : lots A à E faits (avec Codex, relus), F fait. Reste : la vérification de fin d'étape (run complet lancé par toi, comparaison à la main de 3 cinémas).**

## Vue d'ensemble

```text
pour chaque cinéma visible × chaque jour ciné (J → J+2, J+6 le mercredi)
  └─ GET /_/showtimes/theater-{id}/d-{date}/ (+ /p-{n}/ tant que page < totalPages)
       └─ JSON → structs serde → films + séances (version, formats, lien de réservation)
            └─ une transaction : upsert des films, remplacement des séances (cinéma, date)
fin : purge des séances passées, ligne dans scrape_runs, rapport
```

| Lot | En une phrase | Réseau ? | Base ? |
|---|---|---|---|
| A | Lire les fixtures JSON dans des structs Rust | non | non |
| B | Transformer une séance AlloCiné en `VF`/`VO`/`VOST` + formats + lien | non | non |
| C | Télécharger un cinéma pour une date | oui (1 cinéma) | non |
| D | Écrire films et séances en base | oui (1 cinéma) | oui |
| E | Toute la France, en parallèle, sans se faire bloquer | oui (tout) | oui |
| F | Purge, ligne `scrape_runs`, rapport | – | oui |

Ordre : on ne passe au lot suivant que quand le « C'est fini quand » du lot est vrai.

---

## Ce que contiennent les fixtures (vérifié le 2026-10-07)

Fichiers dans `backend/tests/fixtures/` :

| Fichier | Cinéma | `results` | Particularité |
|---|---|---|---|
| `showtimes-C0159-2026-10-06-p1.json` | UGC (multiplexe) | 15 films | `totalPages: 2`, `totalItems: 30` |
| `showtimes-C0159-2026-10-06-p2.json` | idem, page 2 | 15 films | ⚠️ `"page": "2"` est une **chaîne** ici (un nombre en p1) |
| `showtimes-C0015-2026-10-06-p1.json` | Christine Cinéma Club (Art et Essai, Paris) | 10 films | beaucoup de VO/VOST |
| `showtimes-P1434-2026-10-06-p1.json` | Agora (Châteaulin) | 1 film | petit cinéma |
| `showtimes-P0095-2026-10-06-p1.json` | Bonne-Garde | 0 film | jour sans séance : `error: true`, `message: "next.showtime.on"`, `nextDate: "2026-10-07"` |

Trois pièges à retenir :

1. **Jour sans séance = `error: true`**. Ce n'est pas une vraie erreur : le message `next.showtime.on` dit juste « prochaine séance le `nextDate` ». Il faudra le distinguer d'une vraie erreur (lot C).
2. **`pagination.page` change de type** (nombre ou chaîne). Solution simple : ne pas mettre `page` dans ta struct, tu n'en as pas besoin (seul `totalPages` sert).
3. **`movie.languages` peut contenir `null`** (`[null]` vu dans C0015).

---

## A. Fixtures et désérialisation (hors ligne)

### A.1 Fixtures ✅ (2026-10-06)

Rien à faire, voir le tableau ci-dessus.

### A.2 Les structs serde

**Fichier** : `src/showtimes/allocine.rs`. Uniquement des types et du parsing : pas de HTTP, pas de SQL ici.

Ton brouillon actuel a la bonne forme (4 structs) mais les types sont des placeholders. À corriger :

- `Result` est déjà le nom de `std::result::Result` : appelle-la autrement (`MovieShowtimes`, `Entry`…), sinon tu auras des erreurs bizarres partout dans le fichier.
- Faute de frappe : `shotimes` → `showtimes`.
- Ajoute `#[derive(Debug, Deserialize)]` sur chaque struct, et `#[serde(rename_all = "camelCase")]` quand un champ JSON est en camelCase (`internalId`, `startsAt`, `totalPages`…).
- Un champ qui peut être `null` **ou absent** : `Option<T>` + `#[serde(default)]`.
- Ne mets **que** les champs listés ci-dessous : serde ignore tout le reste par défaut.

Ce qu'il faut mettre, struct par struct (chemin JSON → type Rust à choisir) :

**`Response`** (la racine)

| JSON | Exemple | Type Rust |
|---|---|---|
| `error` | `false` | `bool` |
| `message` | `null` / `"next.showtime.on"` | `Option<String>` |
| `nextDate` | `null` / `"2026-10-07"` | `Option<String>` |
| `pagination.totalPages` | `2` | une petite struct `Pagination { total_pages: u32 }` |
| `results` | tableau | `Vec<TaStructEntry>` |

**Entrée de `results`** : `{ movie, showtimes }`

| JSON | Type Rust |
|---|---|
| `movie` | `Movie` |
| `showtimes` | objet aux **clés variables** (`"original"`, `"multiple"`, `"dubbed"`…) → valeur `Vec<Showtime>` |

Indice pour `showtimes` : quelle collection de `std::collections` associe une clé `String` à une valeur ? Il y en a deux ; prends celle qui garde les clés **triées** (ordre stable dans les tests et les logs). Beaucoup de groupes sont des tableaux vides : c'est normal.

**`Movie`** (seulement ce qui va dans la table `movies`)

| JSON | Exemple | Colonne `movies` |
|---|---|---|
| `internalId` | `1000023992` | `id` (INTEGER) |
| `title` | `"Ni vue, ni connue"` | `title` (+ `title_search` au lot D) |
| `originalTitle` | | `original_title` |
| `poster.url` | `"https://fr.web.img6.acsta.net/…jpg"` | `poster_url` (`poster` peut être `null`) |
| `synopsis` | | `synopsis` |
| `runtime` | `"1h 55min"` | `runtime_min` (converti en A.3) |
| `languages` | `["FRENCH"]`, `[null]` | pas en base, sert au mapping de version (lot B) → `Vec<Option<String>>` |
| `genres[].translate` | `"Comédie"` | `genres` (JSON) |
| `countries[].localizedName` | `"France"` | `countries` (JSON) |
| `credits[]` où `position.name == "DIRECTOR"` → `person.firstName` / `lastName` | `Marc` / `Fitoussi` | `directors` (JSON) |
| `cast.edges[].node.actor.firstName` / `lastName` | `Isabelle` / `Huppert` | `cast_members` (JSON) |
| `releases[0].releaseDate.date` | `"2026-10-07"` | `release_date` |
| `releases[0].certificate.label` | `"Tout public"` | `certificate` |
| `data.productionYear` | `2026` | `production_year` |

Les objets imbriqués (`poster`, `credits[].person`, `cast.edges[].node.actor`…) = une petite struct par niveau. C'est verbeux mais simple. Si une forme te bloque, mets temporairement `serde_json::Value`, regarde avec `dbg!`, puis remplace par une struct.

**`Showtime`**

| JSON | Exemple | Type Rust / usage |
|---|---|---|
| `internalId` | `82306178985` | dépasse `u32` → quel entier ? Stocké en `TEXT` (`showtimes.id`) |
| `startsAt` | `"2026-10-06T20:15:00"` | `String` → `starts_at` |
| `diffusionVersion` | `"ORIGINAL"`, `"DUBBED"` | `String` (lot B) |
| `tags` | `["Localization.Version.French", …]` | `Vec<String>` (lot B) |
| `projection`, `sound`, `picture`, `experience` | `["DIGITAL"]`, `["DOLBY_71"]`, `null` | `Option<Vec<String>>` (lot B, formats) |
| `data.ticketing[]` → `{ urls: [String], type, provider }` | voir B.4 | lien de réservation |

`type` est un mot réservé en Rust : `#[serde(rename = "type")] kind: String`.

**Vérifier A.2** : un test qui lit `tests/fixtures/showtimes-C0159-2026-10-06-p1.json` (`include_str!` ou `std::fs::read_to_string`), appelle `serde_json::from_str::<Response>` et fait `.unwrap()`. Puis la même chose sur **les 5 fixtures** (une boucle ou 5 tests). Tant qu'un `unwrap` panique, le message de serde dit quel champ et à quelle ligne.

### A.3 Deux fonctions pures

**Fichier** : `src/showtimes/mapping.rs`.

- [x] `runtime_minutes(&str) -> Option<u32>`
  - `"1h 55min"` → `Some(115)` ; `"0h 00min"` → `None` (durée inconnue, pas 0) ; `""` ou `"abc"` → `None`, **jamais de panique**.
  - Indice : `split_once('h')`, `trim`, `trim_end_matches("min")`, `parse::<u32>().ok()?`.
- [x] `full_name(first: Option<&str>, last: Option<&str>) -> Option<String>`
  - les deux → `"Isabelle Huppert"` ; un seul → celui-là ; aucun → `None`.

Un test par cas listé.

### A.4 Tests de contenu

- [x] `C0159` p1 : `results.len() == 15`, `total_pages == 2`, le film `1000023992` s'appelle `"Ni vue, ni connue"` et a au moins une séance avec un `starts_at` et un lien.
- [x] `P0095` : la désérialisation **réussit**, `error == true`, `results` vide, `next_date == Some("2026-10-07")`.

**C'est fini quand** : `cargo test` vert, `cargo clippy --all-targets` sans warning.

---

## B. Mapping version, formats et lien (fonctions pures)

**Fichier** : `src/showtimes/mapping.rs`.

### B.1 Ce qu'on voit dans les fixtures (déjà relevé)

| Groupe | `diffusionVersion` | Tags `Localization.*` | `languages` | Version attendue |
|---|---|---|---|---|
| `multiple` | `DUBBED` | `Version.French` | `FRENCH` | `VF` (film français : `DUBBED` ne veut **pas** dire doublé ici) |
| `multiple` | `DUBBED` | `Version.French` + `Subtitle.French` | `FRENCH` | `VF` (sous-titres français pour un film français = accessibilité) |
| `original` | `ORIGINAL` | `Version.Original` + `Subtitle.French` | `ENGLISH`, `JAPANESE`… | `VOST` |
| `original` | `ORIGINAL` | `Version.Original` | `ENGLISH`… | `VO` |
| `original` | `ORIGINAL` | `Version.Original` | `FRENCH` | `VF` (film français en VO) |
| `original` | `ORIGINAL` | `Version.Original` | `CANTONESE`, `FRENCH` | ? (coproduction : à décider) |

Vu ensuite en réel (P0095, 2026-10-08) : `multiple_sme` / `LOCAL` avec `Localization.Language.French` → `VF`, et `original_st` avec `Showtime.Accessibility.Subtitled` sans `Subtitle.French` → `VOST` (règles ajoutées, voir `SOURCES.md`). Pas encore vu : `dubbed`. Tu pourras en télécharger d'autres plus tard (un gros multiplexe avec un film d'animation doublé, par exemple).

- [x] Recopie ce tableau dans `SOURCES.md` §4.

### B.2 `version`

- [x] `enum Version { Vf, Vo, Vost }` + une méthode `as_str()` qui renvoie `"VF"` / `"VO"` / `"VOST"` (ce qui va en base).
- [x] `fn version(showtime: &Showtime, languages: &[Option<String>]) -> Option<Version>`. Règle de départ à partir du tableau :
  1. tag `Localization.Version.French` → `Vf` ;
  2. sinon tag `Localization.Version.Original` : si `FRENCH` est la **première** langue du film → `Vf` ; sinon `Subtitle.French` présent → `Vost`, absent → `Vo` ;
  3. sinon → `None` : l'appelant fait un `warn!` et **saute** la séance (mieux vaut une séance manquante qu'une fausse version).
- [x] Un test par ligne du tableau B.1 (tu peux construire un `Showtime` à la main dans le test, ou piocher une vraie séance dans une fixture).

### B.3 `formats`

- [x] `fn formats(showtime: &Showtime) -> Vec<&'static str>` qui ne renvoie que des valeurs de la liste de `API.md` : `3D`, `IMAX`, `4DX`, `ScreenX`, `Dolby Cinema`, `Dolby Atmos`.
- Dans les fixtures on ne voit que `DIGITAL`, `ANALOG` (projection) et `DOLBY_71` (son) : **aucun** n'est dans la liste, donc `formats` sera souvent `[]`. C'est normal.
- Les valeurs inconnues : `debug!` pour l'instant (le « loggé une seule fois » avec un `HashSet` viendra au lot E, quand il y aura des milliers de séances).
- Pour trouver les vraies valeurs IMAX / 3D, télécharge plus tard une fixture d'un cinéma IMAX (Pathé La Villette, Grand Rex…) et regarde `experience`, `picture`, `tags`.
- En base : `serde_json::to_string(&formats)`.

### B.4 Lien de réservation

Chaque séance a deux entrées dans `data.ticketing` :

- `provider: "default"` : le site du cinéma (ex. `https://www.ugc.fr/reservationSeances.html?id=…`) ;
- `provider: "relay"` : un intermédiaire (`relay.mvtx.us`).

- [x] `fn booking_url(showtime: &Showtime) -> Option<&str>` : le premier `urls[0]` de `default` en `DESKTOP`, sinon celui de `relay`, sinon `None`. Écris la règle en commentaire au-dessus.

### B.5 Accessibilité (noter seulement)

- [x] Dans `SOURCES.md` : les tags vus (`Showtime.Accessibility.Accessible`, `Theater.Service.DisabledAccess`, `SME` dans l'URL relay). Rien en base pour l'instant.

**C'est fini quand** : tests de `version` (chaque ligne de B.1), `formats` (au moins `[]` sur une vraie séance) et `booking_url` (cas `default`, cas sans ticketing) verts.

---

## C. Un cinéma, une date, en réseau

**Fichiers** : `src/showtimes/allocine.rs` (URL + fetch), `src/showtimes/mod.rs` (fonction appelée par la commande), `src/cli.rs` et `src/main.rs` (sous-commande).

- [x] **URL** : `fn showtimes_url(cinema_id: &str, date: &str, page: u32) -> String`
  - page 1 → `https://www.allocine.fr/_/showtimes/theater-C0159/d-2026-10-07/`
  - page 2 → `…/d-2026-10-07/p-2/`
  - toujours le `/` final (sinon redirection 301, comme pour les listes de cinémas). Un test par cas.
- [x] **En-têtes** : `allocine_request` envoie `Referer` et `Accept: application/json`.
- [x] **Téléchargement** : `client::fetch_with_retries(&url, &RETRY_DELAYS, || …)` renvoie des `Bytes` → `serde_json::from_slice::<Response>`.
- [x] **Erreur applicative** : `error == true` →
  - `message == Some("next.showtime.on")` → pas une erreur, 0 film ;
  - autre message → `anyhow::bail!` avec le message, l'ID et la date.
- [x] **Pagination** : page 1, lire `total_pages`, puis boucle `2..=total_pages` avec une pause fixe entre deux pages et concaténer les `results`.
- [x] **Sous-commande** : `Scrape` accepte `--cinema` et `--date`. Depuis le lot D, les résultats sont enregistrés en base après le téléchargement.

403 et 429 ne sont déjà pas retentés (`is_retryable`) : on s'en occupe au lot E.

**C'est fini quand** : `cargo run -- scrape --cinema C0159 --date <aujourd'hui>` affiche le même nombre de films que la page AlloCiné du cinéma pour ce jour, et `--cinema P0095` sur un jour vide affiche 0 sans erreur.

---

## D. Écriture en base

**Fichiers** : `src/showtimes/mod.rs`, `src/showtimes/db.rs`, `Cargo.toml` (`chrono` et `chrono-tz` pour `Europe/Paris`).

### D.1 Les dates à scraper

- « Aujourd'hui » = aujourd'hui **à Paris**, pas en UTC (bug n°1 de l'ancien front). Crate : `jiff` (`Zoned::now().in_tz("Europe/Paris")`) ou `chrono` + `chrono-tz`.
- [x] `fn cine_dates(today: Date) -> Vec<Date>` **pure** (on lui passe la date, elle ne lit pas l'horloge) : J, J+1, J+2, et en plus J+6 si `today` est un mercredi (`PLAN.md`).
- [x] Tests : un lundi → 3 dates ; un mercredi → 4 dates ; un 30 décembre → passage d'année correct.
- La colonne `showtimes.date` = la date **demandée** à AlloCiné, pas la date de `starts_at` (une séance à 0h30 appartient au jour ciné précédent).

### D.2 Films (upsert)

- [x] `INSERT INTO movies (…) VALUES (…) ON CONFLICT(id) DO UPDATE SET title = excluded.title, …`
- Colonnes à remplir : le tableau `Movie` de A.2. `title_search = text::normalize(title)`. `updated_at` = maintenant.
- Colonnes JSON (`genres`, `directors`, `cast_members`, `countries`) : `serde_json::to_string(&vec)`.
- ⚠️ Ne mets **pas** `tmdb_id`, `rating`, `backdrop_url`, `trailer_url`, `tmdb_synced_at` dans la requête : ils viendront de TMDB et ne doivent pas être écrasés.

### D.3 Séances (remplacement)

Pour un couple (cinéma, date), dans **une** transaction :

1. upsert des films (clé étrangère : le film doit exister avant la séance) ;
2. `DELETE FROM showtimes WHERE cinema_id = ? AND date = ?` ;
3. `INSERT` de chaque séance : `id` (= `internalId` en texte), `cinema_id`, `movie_id`, `date`, `starts_at`, `version`, `formats`, `booking_url`.

Une séance qui disparaît d'AlloCiné = annulée : le `DELETE` puis `INSERT` s'en occupe.

- [x] Tout le réseau du couple (cinéma, date) est terminé **avant** `pool.begin()` (transaction courte, `SOBRIETE.md`).

### D.4 Tests sur base en mémoire

Modèle : `save_matches_replaces_previous_cnc_data` dans `src/cinemas/cnc.rs`.

- [x] Deuxième passage sans une séance → elle est supprimée.
- [x] Les séances d'une **autre date** du même cinéma restent.
- [x] Un film avec un `tmdb_id` déjà rempli le garde après upsert.

**C'est fini quand** : après `scrape --cinema C0159` (avec écriture cette fois) :

```sql
select count(*), count(distinct movie_id) from showtimes where cinema_id = 'C0159' and date = '…';
select version, count(*) from showtimes group by 1;
select count(*) from showtimes where booking_url is null;
```

donnent des nombres cohérents avec la page AlloCiné.

---

## E. Toute la France : concurrence, débit, coupe-circuit

**Fichier** : `src/showtimes/mod.rs`. Dépendances probables : `futures`, peut-être `governor`.

- [x] **E.1 Liste des cibles** : requête SQL des cinémas **visibles** (`lat IS NOT NULL` et vus depuis moins de 14 jours, `API.md` « Cycle de vie »).
- [x] **E.2 Estimation avant mesure** : 3 100 cinémas × 3 dates × 1,2 page en moyenne = 11 160 requêtes, soit environ **62 minutes à 3 req/s**. Le mercredi ajoute environ 1/7 de date en moyenne (environ 65 minutes). Cela dépasse les 30–40 minutes prévues dans `PLAN.md`; retries, latences et écritures peuvent encore allonger le run. Détail dans `SOBRIETE.md`.
- [x] **E.3 Parallélisme** : quatre cinémas au plus sont traités simultanément avec `tokio::task::JoinSet`.
- [x] **E.4 Débit global** : un limiteur partagé espace toutes les tentatives d'au moins 333 ms, y compris pages et retries.
- [x] **E.5 Coupe-circuit** : un compteur atomique partagé suit les réponses 403/429 consécutives, se réinitialise sur une réponse réussie et arrête les nouveaux appels à cinq.
- [x] **E.6 Erreurs isolées** : une erreur réseau ou SQLite marque le cinéma/date en échec, laisse ses données existantes en place et n'interrompt pas les autres tâches.
- [x] **E.7 Écritures** : chaque cinéma/date écrit dans sa transaction courte; le pool garde son `busy_timeout`.
- [x] **E.8 Option `--department 75`** pour lancer un département ciblé.

**C'est fini quand** : `scrape --department 75` passe sans erreur, puis toute la France lancée par toi avec `caffeinate -i` et `/usr/bin/time -l` (durée, mémoire max, CPU → `SOBRIETE.md`).

---

## F. Purge, suivi des runs, rapport

**Fichier** : `src/showtimes/mod.rs`. Modèle : `log_import_report` / `import_report` dans `src/cinemas/mod.rs`.

- [x] **F.1 `scrape_runs`** : `INSERT` au début (`kind = 'showtimes'`, `started_at`), garder l'`id`, puis `UPDATE` de **cette** ligne à la fin (`finished_at`, `ok_count`, `error_count`). Un run interrompu garde `finished_at = NULL` (utile pour `/api/meta`). Décide si `ok_count` compte des cinémas ou des couples (cinéma, date) et écris-le en commentaire.
- [x] **F.2 Purge** en fin de run : `DELETE FROM showtimes WHERE date < ?` avec « aujourd'hui à Paris » (la même fonction qu'en D.1). Test : hier supprimé, aujourd'hui et demain gardés. Films sans séance : on les garde pour l'instant (TMDB les aura peut-être enrichis).
- [x] **F.3 Rapport** (`info!`) : cinémas ok / en erreur, films, séances par version, séances sans lien, durée, coupe-circuit déclenché ou non.

Fait le 2026-10-08 :

- `ok_count` / `error_count` = couples **(cinéma, date)** (l'unité de transaction ; un cinéma peut réussir J et rater J+1). Les cinémas non lancés à cause du coupe-circuit comptent en erreur (leurs dates ne sont pas rafraîchies).
- `kind = 'showtimes'` pour un run France entière, `'showtimes_partial'` pour `--cinema` / `--department` : `/api/meta` ne lira que les runs complets.
- La commande sort en erreur (code ≠ 0, utile pour le cron) si le coupe-circuit s'est déclenché ou si aucun couple n'a réussi.
- Rapport : deux lignes `info!` : compteurs du run (cinémas, couples, séances écrites, purge, durée, coupe-circuit) puis état de la base (séances, films, cinémas, VF / VO / VOST, sans lien).

**C'est fini quand (fin de l'étape 2, à cocher dans `PLAN.md`)** :

- 3 cinémas comparés à la main avec le site AlloCiné (C0159, C0015, P1434) : mêmes films, même nombre de séances, versions justes.
- Aucun film manquant sur les gros cinémas (pagination) : `select cinema_id, date, count(distinct movie_id) from showtimes group by 1, 2 order by 3 desc limit 5`.
- Durée, mémoire et nombre de requêtes d'un run complet notés dans `SOBRIETE.md`.
