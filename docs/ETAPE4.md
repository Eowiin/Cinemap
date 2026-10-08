# Étape 4 : enrichissement TMDB

Écrite par Claude le 2026-10-08, à ta demande. Code : `backend/src/tmdb/` (`api.rs` appels HTTP, `matching.rs` fonctions pures, `mod.rs` orchestration et base).

## Ce qu'il te reste à faire

1. Créer un compte sur https://www.themoviedb.org, puis **Paramètres > API** : demander une clé (usage personnel, non commercial).
2. Dans `backend/.env`, ajouter **l'une des deux** valeurs fournies par TMDB (le code reconnaît les deux) :

   ```
   TMDB_API_KEY=<clé d'API v3, 32 caractères>
   # ou le « jeton d'accès en lecture » (long, commence par eyJ)
   ```

3. Lancer `cargo run --release -- tmdb` depuis `backend/` (≈ 344 films pour Paris, 2 à 3 requêtes par film à ≤ 10 req/s : 1 à 2 min).
4. Vérifier :

   ```sql
   select count(*), count(tmdb_id), count(trailer_url), count(backdrop_url), count(rating)
   from movies where tmdb_synced_at is not null;
   select title, original_title, production_year from movies
   where tmdb_synced_at is not null and tmdb_id is null limit 20;   -- les films non croisés : à regarder à l'œil
   select * from scrape_runs where kind = 'tmdb' order by id desc limit 1;
   ```

   Puis ouvrir 3 ou 4 fiches dans le front et vérifier que la bande-annonce est bien celle du film.

## Premier run (2026-10-08, base de Paris)

- 344 films, **312 croisés (91 %)**, 32 introuvables, 0 erreur, **77 s**.
- Sur les 312 : 266 bandes-annonces, 296 images de fond, 242 notes (les autres ont moins de 10 votes).
- 12 croisements tirés au hasard et comparés à la fiche TMDB : **tous corrects**, y compris un écart d'un an (*Eephus* 2024/2025) et un titre français différent (*Soundtrack to a Coup d'État* → *Bande-son pour un coup d'État*).
- Les introuvables sont surtout des films rares, des courts, des documentaires et des versions spéciales (« Director's Cut », « Extended »). Un seul cas évitable : *Deux ou trois choses que je sais d'elle*, que TMDB écrit « 2 ou 3 choses… ». Pas la peine de complexifier le croisement pour ça.
- Les tests de désérialisation écrits à la main correspondent bien aux vraies réponses : aucune erreur de format.

## Fonctionnement

- **Quels films** : ceux qui ont une séance à venir, jamais synchronisés ou synchronisés il y a plus de 7 jours (`--all` force tout).
- **Croisement** (`matching::best_match`) : recherche par titre original, puis par titre français s'il diffère. Un résultat est accepté si son titre (français ou original) est **exactement** l'un des nôtres une fois normalisé (minuscules, sans accents, lettres et chiffres seulement), et si son année de sortie est à ±1 an de l'année de production AlloCiné. Pas de similarité approximative : mieux vaut aucune bande-annonce que celle d'un autre film.
- **Film déjà croisé** : on ne recherche plus, on relit seulement la fiche (la note et les vidéos changent).
- **Introuvable** : `tmdb_synced_at` est quand même rempli, pour ne pas rechercher chaque nuit (nouvel essai dans 7 jours).
- **Bande-annonce** : YouTube, `Trailer` avant `Teaser`, français avant le reste, officielle avant le reste.
- **Note** : `vote_average` arrondi à 0,1, `null` sous 10 votes.
- **Clé refusée (401)** : arrêt immédiat avec un message clair, au lieu de 344 erreurs.
- **Run** : une ligne `scrape_runs` `kind = 'tmdb'` (`ok_count` = croisés + introuvables, `error_count` = erreurs réseau ou de format).
- La clé n'apparaît jamais dans les messages d'erreur (elle est ajoutée à la requête, pas à l'URL loggée).
- `reqwest` a reçu la feature `query` (dans la 0.13, `.query()` est derrière cette feature).

Le scraper (`upsert_movie`) ne touche pas aux colonnes TMDB : un test le vérifiait déjà (`save_showtimes_replaces_date_and_preserves_tmdb_data`).

Limite connue : les tests de désérialisation reposent sur des réponses **écrites à la main** d'après la documentation TMDB, pas sur de vraies réponses (pas de clé). Si le premier run révèle un écart, enregistre une vraie réponse dans `tests/fixtures/` et remplace.

## Questions pour toi

1. Dans `tmdb/api.rs`, `Tmdb::get` prend `&mut self`, alors que la closure passée à `fetch_with_retries` utilise `self.authorized(...)` et `self.client`. Pourquoi le compilateur l'accepte-t-il ? Qu'est-ce qui changerait si la closure devait modifier `self.next_request_at` ?
2. `best_match` renvoie `Option<&'a SearchResult>` avec la même durée de vie que `candidates`. Que se passerait-il si on écrivait `-> Option<&SearchResult>` sans annotation, avec les deux paramètres de type référence (`candidates` et `titles`) ?
3. `is_unauthorized` parcourt `error.chain()` et fait un `downcast_ref::<reqwest::Error>()`. Pourquoi ne pas simplement faire `error.downcast_ref::<reqwest::Error>()` sur l'erreur reçue ?
