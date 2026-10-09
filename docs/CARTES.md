# Cartes illimitées (feuille de route détaillée)

Même principe que `ETAPE2.md` : le **quoi** et le **pourquoi**, avec des indices ; le **comment**, c'est toi (sauf si tu me délègues un lot, dis-le).
Chaque lot : **fichier à toucher → quoi y mettre → comment vérifier**. On ne passe au lot suivant que quand son « C'est fini quand » est vrai.
Références : `API.md` « Proposition : cartes illimitées » (schéma, contrat, décisions du 2026-10-06), `cinemas/cnc.rs` (croisement par nom, à réutiliser).

Rédigé le 2026-10-08. **Où tu en es (2026-10-09) : lots A à F faits par Claude** (délégués), voir « Réalisé le 2026-10-09 » en bas. Reste le premier remplissage en prod et le lot G. Priorité : UGC Illimité, puis Pathé CinéPass, puis d'autres cartes si on trouve des listes exploitables.

## Besoin

Savoir dans quels cinémas une carte d'abonnement est acceptée, afficher un badge sur ces cinémas et filtrer cinémas, films et séances selon **mes** cartes (choisies dans le navigateur, pas de compte). Le badge est au niveau du **cinéma** : les restrictions par séance (avant-premières exclues, suppléments IMAX / 3D) sont ignorées (décision du 2026-10-06).

## Sources (vérifiées le 2026-10-08)

| Carte | Source | Format | Contenu | Croisement |
|---|---|---|---|---|
| UGC Illimité | `https://www.ugc.fr/cinemas-acceptant-ui.html` | HTML statique, une seule page | **145** cinémas (UGC, mk2, partenaires), nom + adresse + `CP VILLE` ; 59 à Paris | code postal + nom |
| Pathé CinéPass, réseau Pathé | `https://www.pathe.fr/api/cinemas` | JSON, ~375 Ko | **78** cinémas Pathé (pas les partenaires), adresse, CP, **coordonnées GPS** | distance GPS + nom |
| Pathé CinéPass, partenaires | `https://www.pathe.fr/media/files/conditions/Reseau%20CinePass-CineCartes.pdf` | PDF de 3 pages (export Excel) | ~140 lignes dont ~60 « Cinéma indépendant » ; **pas d'adresse ni de code postal**, juste une « agglomération » (`PARIS - ILE DE France` pour toute l'IDF) | à la main, une fois (CSV) |

Ce que j'ai observé en ouvrant les fichiers :

- **UGC** : chaque cinéma est un bloc `div.item--cinema-content` contenant `div.color--white` (le nom, en majuscules ou non : `UGC Ciné Cité Bercy`, `GRAND ECRAN ESTER`) et `div.color--blue-grey` (adresse, `<br>`, puis `75012&nbsp;PARIS`). Les régions (`region-N`) sont des accordéons fermés, mais tout est déjà dans le HTML : une seule requête suffit.
- **UGC, croisement par code postal** contre notre base (France entière) : 142 sur 145 ont au moins un cinéma AlloCiné avec le même code postal. Les 3 restants :
  - `UGC Ciné Cité Noisy-le-Grand` a un code **CEDEX** (`93193`) ;
  - `GRAND ECRAN ESTER` et `HORIZON GRAND ECRAN` (Limoges, `87100`) ne sont pas dans notre base sous ce code postal.
  - Il faut donc un repli : même **département**, avec un seuil de similarité plus strict.
- **Pathé JSON** : un tableau de cinémas. Champs utiles : `name`, `status` (booléen, `false` pour Le Cézanne à Aix, probablement fermé ou en travaux), `theaters[]` (1 à 3 entrées) avec `addressZip`, `addressCity` et `gpsPosition`.
  - **Piège** : dans `gpsPosition`, `x` est la **latitude** et `y` la **longitude** (`{"x": 47.479, "y": -0.551}` à Angers).
  - **Piège** : beaucoup de codes postaux sont des CEDEX (`77705` Disney Village, `94521` Belle Épine, `84085` Cap Sud, `44818` Atlantis, `77566` Carré Sénart). D'où le croisement par **distance GPS** plutôt que par code postal.
  - Le JSON contient aussi des cinémas absents du PDF (Le Renoir à Aix, Pathé Île Seguin). Je pense qu'ils acceptent quand même la carte, puisqu'ils font partie du réseau, mais c'est à confirmer.
- **Pathé PDF** : les 4 formules (CinéPass -26, CinéPass, Silver, Gold) sont « Accepté » sur **toutes** les lignes, donc une seule carte `pathe_cinepass` suffit. La colonne CinéCartes (cartes prépayées) varie, mais on l'ignore pour l'instant. Lire un PDF en Rust est lourd et fragile (crates `lopdf` / `pdf-extract`), pour une liste qui change quelques fois par an : on ne l'automatise **pas**.

## Décisions (proposées le 2026-10-08, ajustées le 2026-10-09 sur les vraies données)

1. **Modèle générique** : tables `cards` et `cinema_cards`, comme dans `API.md`. Ajouter une carte = une source (module Rust) **ou** des lignes dans le CSV manuel.
2. **Une source = une fonction pure + un téléchargement** : `parse_xxx(&str) -> Vec<CardCinema>` testée sur une fixture, puis un croisement commun.
3. **Croisement UGC** : candidats au **même code postal**. S'il n'y en a aucun, candidats du **même département**, avec un seuil plus haut (0,92 au lieu de 0,85, à ajuster sur les vraies données). Ensuite, même algorithme que le CNC : similarité de nom, meilleur score attribué en premier, un cinéma par entrée.
4. **Croisement Pathé (JSON)** : candidats à **moins de 500 m** (haversine sur nos `lat`/`lng`). Le nom sert à départager, avec un seuil plus bas (les noms Pathé et AlloCiné sont proches). `status = false` : ignoré.
5. **Partenaires Pathé et corrections** : `docs/data/cartes.csv` (`card_id,cinema_id,commentaire`), rempli **une fois à la main** à partir du PDF. Je peux préparer une première version à vérifier. Le CSV est appliqué **après** les sources automatiques. Un `cinema_id` inconnu est loggé (`warn!`) et ignoré.
6. **Garde-fou** : si une source renvoie 0 cinéma, ou **moins de la moitié** du nombre de liens actuels pour cette carte, on garde les anciens liens et on loggue un `warn!`. Une page qui change de structure ne doit pas effacer UGC du site en silence.
7. **Quand** : sous-commande `import-cards`, lancée aussi à la fin d'`import-cinemas` (le mardi, timer hebdo). Une source en erreur ne fait pas échouer l'import des cinémas (même règle que le CNC).

## Vue d'ensemble

```text
import-cards
  pour chaque source automatique (UGC, Pathé JSON) :
    GET la page → parse (fonction pure) → Vec<CardCinema { name, postal_code, lat/lng? }>
    croisement avec les cinémas visibles → Vec<(cinema_id, score)>
    garde-fou (vide / chute > 50 %) → sinon une transaction : DELETE liens de la carte, INSERT
  puis docs/data/cartes.csv (ajouts manuels)
  log : par carte, entrées / croisées / non croisées (+ warn! de chaque non croisée)
```

| Lot | En une phrase | Réseau ? | Base ? |
|---|---|---|---|
| A | Migration, module commun de similarité, sous-commande vide | non | oui |
| B | UGC : parse de la fixture, croisement, enregistrement | oui (1 page) | oui |
| C | Contrat API (`cards` partout) et endpoints | non | oui |
| D | Pathé : JSON (GPS) + CSV des partenaires | oui (1 requête) | oui |
| E | Front (Claude) : « mes cartes », badge, filtre | – | – |
| F | Prod : branchement dans `import-cinemas`, vérification | oui | oui |
| G | Autres cartes (recherche, puis une source ou des lignes CSV) | – | – |

---

## Lot A : socle

- [x] **Migration** `backend/migrations/<horodatage>_create_cards.up.sql` (+ `.down.sql`) : les deux tables d'`API.md`. Ajouter `card_id` dans un index de `cinema_cards` (la PK `(cinema_id, card_id)` sert pour « les cartes d'un cinéma », il en faut un sur `card_id` pour « les cinémas d'une carte »). Les lignes de `cards` sont insérées par le code (upsert au début de l'import), pas par la migration : ajouter une carte ne demande pas de migration.
  - Indice : `ON DELETE CASCADE` ne marche que si les clés étrangères sont activées. sqlx les active par défaut pour SQLite (`SqliteConnectOptions::foreign_keys`), vérifie-le une fois avec un test.
- [x] **Module commun** : sortir de `cnc.rs` `comparable_name`, `similarity`, `word_containment` et la boucle « meilleur score d'abord, chacun utilisé une fois » dans `backend/src/matching.rs` (ou `cinemas/matching.rs`). `cnc.rs` l'appelle ensuite. Les tests CNC existants doivent passer **sans modification** : c'est ta garantie que le déplacement n'a rien cassé.
  - Indice : la boucle d'attribution peut devenir générique, par exemple `fn assign<'a, A, B>(pairs: Vec<(&'a A, &'a B, f64, f64)>, key_a, key_b) -> Vec<…>`. Ne te sens pas obligé : une version copiée-adaptée avec un TODO est acceptable si les génériques te bloquent. On en parlera à la relecture.
  - Les mots génériques diffèrent un peu : « ugc », « pathe », « mk2 », « gaumont » ne doivent **pas** être retirés (ils distinguent « UGC Montparnasse » de « Pathé Montparnasse »), alors que « ciné cité » apparaît des deux côtés et peut rester.
- [x] **Sous-commande** `ImportCards` dans `cli.rs`, et un module `backend/src/cards/mod.rs` avec `pub async fn import_cards(pool, client) -> anyhow::Result<()>`, vide pour l'instant (log « rien à faire »).
- [x] `cargo sqlx prepare` puis commit de `.sqlx/` (la CI est en `SQLX_OFFLINE=true`).

**C'est fini quand** : `cargo run -- import-cards` tourne, `sqlite3 … ".schema cinema_cards"` montre la table et son index, et les tests CNC passent sans changement.

## Lot B : UGC Illimité

- [x] **Fixture** : `curl -sL https://www.ugc.fr/cinemas-acceptant-ui.html -o backend/tests/fixtures/ugc-illimite-2026-10-08.html` (~137 Ko).
- [x] **Parse** `cards/ugc.rs` : `pub fn parse_ugc(html: &str) -> anyhow::Result<Vec<CardCinema>>` avec `scraper` (déjà en dépendance, comme pour les départements AlloCiné).
  - Sélecteurs : `div.item--cinema-content`, puis dans chaque bloc `div.color--white` (nom) et `div.color--blue-grey` (adresse).
  - Le code postal : 5 chiffres suivis d'une espace insécable (`&nbsp;`, devenue `\u{a0}` après parsing). Regarde si `text::get_postal_code` convient ou s'il faut une variante.
  - Une entrée sans code postal : `warn!` et on la saute (il n'y en a aucune aujourd'hui).
  - Test : 145 entrées, 59 en `75xxx`, et trois entrées vérifiées champ par champ (`UGC Ciné Cité Les Halles` / `75001`, `MK2 BEAUBOURG` / `75003`, `GRAND ECRAN ESTER` / `87100`).
- [x] **Croisement** `cards/matching.rs` (ou dans le module commun) : `fn match_by_postal_code(cinemas: &[CardTarget], entries: &[CardCinema]) -> Vec<CardMatch>` **pure**. Repli par département comme dans la décision 3. Attention au département : `2A`/`2B` pour la Corse, et `97x` en 3 chiffres pour les DOM. `text::department_from_insee` gère peut-être déjà ça, sinon pars de la colonne `department` des cinémas.
  - Tests avec des cinémas écrits à la main : un cas simple, deux UGC dans le même arrondissement (« UGC Montparnasse » et « UGC Rotonde » en `75006`), un CEDEX qui passe par le repli, un nom sans aucun candidat.
- [x] **Enregistrement** : upsert de la ligne `cards` (`ugc_illimite`, `UGC Illimité`, URL, `updated_at`), garde-fou (décision 6), puis transaction `DELETE FROM cinema_cards WHERE card_id = ?` et les `INSERT`. Test sur base en mémoire : un second import qui renvoie 0 entrée **ne touche pas** aux liens.
- [x] **Run réel** sur ta base France.

**C'est fini quand** (requêtes à me coller avant la relecture, avec `build`/`clippy`/`fmt`/`test`) :

```sql
select count(*) from cinema_cards where card_id = 'ugc_illimite';           -- attendu : ~140 sur 145
select c.name, c.postal_code from cinema_cards cc join cinemas c on c.id = cc.cinema_id
where card_id = 'ugc_illimite' and c.postal_code like '75%' order by c.postal_code;  -- 59 lignes, à relire à l'œil
select cinema_id, count(*) from cinema_cards group by cinema_id having count(*) > 1;   -- vide tant qu'il n'y a qu'une carte
```

Ainsi que la liste des non croisés (le `warn!`), à relire **un par un** sur AlloCiné.

## Lot C : contrat et API

- [x] Reporter la proposition d'`API.md` dans le contrat : type `Card`, `cards: string[]` dans `CinemaSummary`, `cards: Card[]` dans `GET /api/meta`, paramètre `cards=ugc_illimite,pathe_cinepass` (« au moins une ») sur `/api/cinemas`, `/api/movies` et `/api/movies/{id}/showtimes`, 400 `bad_request` pour un id inconnu. Puis supprimer le bloc « Proposition ». **Je peux faire ce point** (c'est de la doc), à toi de le relire.
- [x] `cards` dans `CinemaSummary` : un `group_concat(card_id)` dans la requête des cinémas, ou une seconde requête `cinema_id → cartes` fusionnée en Rust. À toi de choisir : mesure les deux avec `oha` (comme dans `SOBRIETE.md`) si tu hésites.
- [x] Filtre `cards` : `EXISTS (SELECT 1 FROM cinema_cards WHERE cinema_id = c.id AND card_id IN (…))`. Avec sqlx et une liste de longueur variable, regarde `QueryBuilder` ou l'astuce `json_each(?)` (on passe la liste en JSON) : la seconde garde `query!` vérifié à la compilation.
- [x] Tests d'API sur `api_seed.sql` enrichi de quelques liens : filtre seul, combiné avec `art_et_essai` et la géoloc, id inconnu → 400, `movies` dont `cinema_count` ne compte que les cinémas filtrés.

**C'est fini quand** : `curl 'localhost:3000/api/cinemas?cards=ugc_illimite' | jq length` donne le même nombre que le `count(*)` du lot B (restreint aux cinémas visibles), et `curl '…/api/meta' | jq .cards` liste la carte.

## Lot D : Pathé CinéPass

- [x] **Fixture** : `curl -sL https://www.pathe.fr/api/cinemas -o backend/tests/fixtures/pathe-cinemas-2026-10-08.json`.
- [x] **Parse** `cards/pathe.rs` : structs serde réduites aux champs utiles (`name`, `status`, `theaters[].{addressZip, addressCity, gpsPosition.{x, y}}`), renommés proprement (`#[serde(rename = "x")] lat`). Test : 78 entrées, 77 avec `status = true`, Angers à `(47.479464, -0.550977)`.
  - Un cinéma avec plusieurs `theaters` : prendre le premier ? Regarde les deux cas dans la fixture (`grep -c` ou `jq '.[] | select(.theaters | length > 1)'`) avant de décider.
- [x] **Croisement par distance** : `fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64` (pure, testée sur deux points connus), candidats à moins de 500 m, puis le nom pour départager. Pas de crate nécessaire pour ça.
  - Vérifie La Géode et Pathé La Villette (même site, 75019), ainsi que Pathé BNP Paribas, qui s'appelle peut-être autrement chez AlloCiné.
- [x] **CSV des partenaires** `docs/data/cartes.csv` : `card_id,cinema_id,commentaire`. Lecture avec `csv` + serde (déjà en dépendance, utilisé par `geocode.rs`) ; `include_str!` comme `departments.rs` si tu veux le CSV dans le binaire (sinon il faut le déployer à côté), appliquée après les sources, `warn!` sur un `cinema_id` inconnu. Les lignes du CSV sont **ajoutées**, elles ne remplacent rien. Pour retirer un mauvais croisement automatique, il faudra une colonne `action` (`add` / `remove`) : à ajouter seulement si le cas se présente.
  - Je peux te préparer le CSV des ~60 partenaires (PDF → identifiants AlloCiné), à vérifier ensuite par toi.

**C'est fini quand** : `select count(*) from cinema_cards where card_id = 'pathe_cinepass'` donne ~75 (réseau) + ~60 (partenaires), et les 30 lignes IDF relues à l'œil.

## Lot E : front (Claude)

- [x] Types (`Card`, `cards` dans `CinemaSummary`), lecture de `meta.cards`.
- [x] « Mes cartes » : cases à cocher (réglages ou barre de filtres), gardées en `localStorage` (même mécanisme que les futurs favoris).
- [x] Badge sur les cinémas (liste, fiche, popup de carte) qui acceptent une de mes cartes, et filtre « seulement mes cartes » (paramètre `cards` dans l'URL, pour les liens partagés).
- [x] Mention de la source et de la date (`updated_at`) sur la fiche cinéma : « D'après la liste UGC du 08/10 ».

## Lot F : prod

- [x] `import_cards` appelé à la fin d'`import-cinemas` (après le CNC), erreur → `warn!` sans faire échouer l'import.
- [x] Rien à changer dans `deploy/systemd/` (le timer hebdo lance déjà `import-cinemas`). Premier remplissage à la main sur le VPS : voir « Prod » plus bas (`systemd-run`, pour charger `/etc/cinemap/cinemap.env`).
- [x] Une ligne `scrape_runs` `kind = 'cards'` ? Utile si on veut voir dans la base quand la liste a été rafraîchie. `cards.updated_at` suffit peut-être : à toi de juger.

## Lot G : autres cartes (plus tard)

À chercher, **sans rien promettre** tant qu'on n'a pas trouvé une liste publique : CGR (abonnement « Le Pass » ?), Cinéville (Bretagne / Pays de la Loire), Kinépolis, les cartes de réseaux indépendants ou régionaux. Pour chacune : la liste existe-t-elle, sous quel format, combien de cinémas ? Moins de ~30 cinémas ou un format pénible → lignes dans `cartes.csv` plutôt qu'un module.

## Pièges

- **Pages qui changent** : UGC peut refaire son site. Le garde-fou (décision 6) évite d'effacer les données ; le `warn!` en prod doit être visible (`journalctl -u cinemap-weekly`).
- **Noms trompeurs** : « Le Lido » existe à Limoges (UGC) et à Paris (partenaire Pathé). Le code postal ou la distance règle le cas, jamais le nom seul.
- **Cinémas masqués** (plus vus depuis 14 jours) : croiser avec tous les cinémas, pas seulement les visibles, pour qu'un cinéma qui réapparaît garde sa carte. L'API filtre déjà les invisibles.
- **Politesse** : 1 requête par source et par semaine, avec le même `User-Agent` et le même `fetch_with_retries` que le reste.


---

## Réalisé le 2026-10-09 (écrit par Claude, lots délégués)

**Résultat sur la base France locale** (3 134 cinémas) :

| Carte | Entrées | Croisées automatiquement | Lignes manuelles | Liens visibles dans l'API |
|---|---|---|---|---|
| UGC Illimité | 145 | 142 | 2 (+ 1 entrée absente d'AlloCiné) | 144 |
| Pathé CinéPass | 77 ouvertes (78 dans le JSON) | 76 | 59 partenaires du PDF (+ Plan de Campagne absent) | 135 |

Pathé annonce « 76 cinémas Pathé et plus de 59 partenaires » : on retombe exactement sur ces chiffres. Les 218 paires automatiques ont été relues une par une, toutes justes après les corrections ci-dessous.

**Écarts avec les décisions proposées, et pourquoi** :

1. **Seuils (décision 3)** : monter le seuil ne protégeait pas. `jaro_winkler` donne un bonus au préfixe commun (« ugc montparnasse » / « ugc rotonde » = 0,854 ; « ugc cite noisy grand » / « ugc cite rosny » = 0,926). Le nom est désormais comparé sur ses **mots distinctifs** (`matching::distinctive_similarity` : on retire les mots communs, inclusion complète = 1), seuil 0,85 partout. Le CNC garde sa `similarity` (ses tests passent sans modification).
2. **Repli par ville, pas par département**, et en **seconde passe** pour toutes les entrées restantes, pas seulement celles sans aucun candidat au même code postal : UGC et AlloCiné ne donnent pas toujours le même code postal (MK2 Odéon 75005 / 75006, Lille 59000 / 59800, 4 Delta 94110 / 94210), et UGC utilise un CEDEX pour Noisy-le-Grand (93193, ville « NOISY-LE-GRAND »).
3. **Noms** : les mots de la ville sont retirés (« Limoges Ester » / « GRAND ECRAN ESTER »), mais on compare aussi les noms entiers (« Arcachon » / « GRAND ÉCRAN ARCACHON ») ; nombres de deux à dix en chiffres (« Cinq Caumartin » / « 5 CAUMARTIN »). À égalité, le nom qui partage la plus grande **part de mots** gagne (Jaccard) : avec Jaro-Winkler, « Langon » partait vers « Grand Écran Langon Rio centre-ville » au lieu du multiplexe.
4. **Pathé (décision 4)** : notre géocodage place mal les multiplexes de centres commerciaux (score ~0,5, de 600 m à 3 km d'écart : Angers, Atlantis, Valenciennes…). Seconde passe jusqu'à **5 km** avec le seuil normal sur le nom ; la première (500 m) garde un seuil de 0,7.
5. **CSV** : colonnes `card_id,cinema_id,source_name,commentaire`. `source_name` = entrée de la source reprise à la main (plus signalée comme non croisée) ; `cinema_id` vide = entrée connue absente d'AlloCiné. Colonne `manual` dans `cinema_cards` : le garde-fou ne compte que les liens automatiques, les lignes manuelles sont réappliquées à chaque import.
6. **Garde-fou** : `import-cards --force` accepte une liste deux fois plus courte (jamais une liste vide). La commande finit en erreur si une source a échoué (après avoir enregistré les autres).
7. **API** : `Card` porte aussi `updated_at` (pour « d'après la liste du 09/10 »). `cards` dans `CinemaSummary` par une sous-requête `json_group_array` sur la clé primaire : +1 ms pour les 3 100 cinémas de `/api/cinemas` (mesuré), pas de seconde requête.
8. **Front** : « mes cartes » = une `StoredList` (comme les favoris), choisies sur l'accueil ; chip « Ma carte / Mes cartes » dans la barre de filtres (paramètre `cards` dans l'URL, liens partageables) ; badges sur la fiche cinéma (toutes les cartes, les miennes en couleur, avec la date des listes), dans les séances d'un film et la popup de la carte (les miennes seulement).

**À vérifier à la main** (commentaires « à confirmer » dans `cartes.csv`) : Studio 6 (Annemasse) = « Cinéma Studio Annemasse » ? Le Cinéma du Trèfle (Dorlisheim) = « Le Trèfle, Molsheim » ? Les Capucins (Coulommiers) = « Les Capucins » de l'IDF ? Et **Pathé Plan de Campagne** absent de notre base : manque-t-il à l'import AlloCiné du 13 ?

**Prod** : la migration passe au redémarrage de `serve` (déploiement). Premier remplissage, sans attendre le timer du mardi :

```bash
# `sudo -u cinemap …` seul ne marche pas : DATABASE_URL est dans /etc/cinemap/cinemap.env,
# que seuls les services chargent.
systemd-run --pty --wait --collect \
  --uid=cinemap --gid=cinemap \
  -p EnvironmentFile=/etc/cinemap/cinemap.env \
  -p WorkingDirectory=/var/lib/cinemap \
  /opt/cinemap/current/cinemap import-cards
```

**Questions pour toi** :

1. `match_by_postal_code` renvoie des `CardMatch<'a>` qui empruntent à la fois `cinemas` et `entries`. Dans `import_cards`, pourquoi a-t-il fallu garder `fetched` (le `Result` de la liste) vivant jusqu'à la fin du tour de boucle, au lieu de faire `match fetch_entries(..).await { Ok(entries) => … }` ? Que dit l'erreur `E0597` ?
2. `assign_best` demande `A: Copy + Eq + Hash`. Les clés sont ici des `usize` (indices) et non des `&CardTarget` : qu'est-ce qui empêche d'utiliser directement des références comme clés d'un `HashSet`, et pourquoi des indices sont-ils plus simples ?
3. `report` utilise `std::ptr::eq(m.entry, entry)` plutôt que `m.entry == entry` (`CardCinema` dérive `PartialEq`). Quelle différence, et dans quel cas `==` donnerait un mauvais résultat ?
