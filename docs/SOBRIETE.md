# Sobriété des ressources

Notes perso : écrire du Rust qui ne gaspille ni CPU, ni mémoire, ni réseau, sans optimiser à l'aveugle.
Complété au fil des relectures de code.

## Règle n°1 : mesurer avant d'optimiser

Avant de changer du code « pour la perf », se demander **où est le vrai coût**. Il y a des ordres de grandeur entre les opérations :

| Opération | Ordre de grandeur |
|---|---|
| Lire une variable, une petite struct | nanosecondes |
| Allouer une `String` / un `Vec` | ~50-100 ns |
| Lire un fichier local / une requête SQLite simple | micro → millisecondes |
| Requête HTTP vers AlloCiné | 100 ms → 1 s |

Exemple vécu (2026-09-30) : `Vec` de 101 départements ou itérateur ? Aucune différence mesurable, parce que derrière il y a une centaine de requêtes HTTP. Le vrai coût est le réseau.

Mesure réelle (2026-10-03, `import-cinemas` en `--release`, interrompu au 34e département) : **788 s réels (dont ~2 min de veille du Mac) pour 0,84 s de CPU**, mémoire max 22 Mo. Le programme passe plus de 99,9 % de son temps à attendre (pauses + réseau) : optimiser le CPU ici ne servirait à rien.

Autre exemple (2026-10-02) : chaque page AlloCiné (~350 Ko) est parsée deux fois (`page_count` puis `parse_department_page`). Quelques millisecondes, contre 3 s de pause volontaire entre deux requêtes : pas la peine de compliquer le code pour ça.

### Taille d'un `Vec` : l'en-tête et le tas (2026-10-05, lecture CNC)

- Un `Vec<T>` compte deux tailles différentes. **L'en-tête** (pointeur, capacité, longueur) fait toujours **24 octets** en 64 bits. **La zone sur le tas** vaut `capacité × size_of::<T>()`.
- `CncCinema` : 8 (`i64`) + 24 + 24 (deux `String`) + 8 (`i64`) + 16 (`Option<i64>`) + 1 (`bool`) = 81, arrondi à **88 octets** pour l'alignement sur 8 (vérifier avec `std::mem::size_of`). Pour 2 060 lignes, cela fait environ 181 Ko, **plus** une allocation par `String` (le texte vit ailleurs sur le tas).
- Sans `with_capacity`, le `Vec` double sa capacité quand il est plein (4, 8, …, 4 096) : une douzaine de réallocations avec une copie à chaque fois, et environ 360 Ko réservés pour 181 Ko utiles.
- Pistes d'économie, de la plus rentable à la moins rentable :
  1. **Supprimer la structure intermédiaire** : remplir directement la `HashMap` par commune au lieu de passer par un `Vec`.
  2. **Moins d'allocations par élément** : un code INSEE de taille fixe (`[u8; 5]`) ; stocker directement le nom normalisé au lieu du nom brut plus sa version normalisée.
  3. **Des types plus petits** (`u16`, `u32`) : environ 64 octets au lieu de 88. Le gain est faible.
- Mais le vrai poste est probablement la `Range` de calamine : **toute la feuille** en mémoire, un `calamine::Data` par cellule et pour toutes les colonnes, sans compter le XLSX et son XML décompressé. Regarder le pic mesuré par `/usr/bin/time -l` (`enrich-cnc`) avant de toucher à quoi que ce soit. Ces données ne vivent que quelques secondes une fois par nuit : si le pic reste de quelques Mo, la piste 1 suffit.
- Mesure (2026-10-05, `enrich-cnc` en `--release`) : **28 Mo** de mémoire max, 0,85 s réel dont 0,08 s CPU (téléchargement inclus). Le XLSX brut est libéré (`drop`) juste après la lecture. Pas d'optimisation nécessaire : le pic est du même ordre que le scraping (22 Mo) et ne dure qu'une seconde.

### Estimation préalable du scraping des séances (2026-10-08)

Ordre de grandeur du lot E, pas une mesure : 3 100 cinémas × 3 dates × 1,2 page moyenne ≈ **11 160 requêtes**. Au plafond global de 3 requêtes/s, le plancher théorique est **3 720 s, soit 62 min**. En ajoutant la date J+6 du mercredi (un jour sur sept), la moyenne monte à environ **11 726 requêtes / 65 min 9 s**. Les délais de réseau et les retries ne sont pas inclus.

Cette estimation dépasse les 30–40 min prévues dans `PLAN.md`. Atteindre 30–40 min demanderait environ 4,7–6,5 requêtes/s, au-dessus du plafond de 3 req/s retenu pour l'instant. Il faudra mesurer un run de département avant de revoir ce budget.

### Outils de mesure

- **Toujours en `--release`** : le mode debug est 10 à 100 fois plus lent, ses chiffres ne veulent rien dire.
- `/usr/bin/time -l cargo run --release -- <cmd>` (macOS) : temps réel, temps CPU, **maximum resident set size** (mémoire max). Sur Linux : `/usr/bin/time -v`.
- `cargo bloat --release` : ce qui gonfle la taille du binaire.
- `dhat` (crate) : nombre et taille des allocations mémoire.
- `cargo flamegraph` : où passe le temps CPU.
- `tracing` avec des spans : durée de chaque étape dans les logs.
- SQLite : `EXPLAIN QUERY PLAN SELECT …` pour vérifier qu'un index est utilisé.

## Réflexes Rust

- **Emprunter plutôt que copier** : paramètre `&str` plutôt que `String`, `&[T]` plutôt que `Vec<T>`. Un `.clone()` pour faire taire le borrow checker est un signal à creuser (parfois légitime, souvent évitable).
- **Itérateurs** : ils sont « à coût nul » (compilés comme une boucle écrite à la main). Éviter les `.collect()` intermédiaires quand on ne fait que reboucler dessus juste après.
- **Pré-allouer** quand on connaît la taille : `Vec::with_capacity(n)`, `String::with_capacity(n)`.
- **Pas d'allocation dans une boucle chaude** si on peut l'éviter : réutiliser un buffer (`buf.clear()` plutôt qu'un nouveau `String` à chaque tour).
- **Construire les objets coûteux une seule fois** : `Selector::parse` (scraper), regex, `reqwest::Client` : une fois, hors de la boucle.
- **Choisir le type selon le stockage réel** : `f32` au lieu de `f64` n'économise rien si la valeur finit dans une colonne SQLite `REAL` (toujours 8 octets) ; on perd juste de la précision.
- **Transactions courtes** : ne jamais garder une transaction SQLite ouverte pendant un appel réseau (le verrou d'écriture bloque tous les autres écrivains pendant ce temps).
- **`Cow<str>`** quand une fonction renvoie « parfois le texte d'origine, parfois une version modifiée » (ex. normalisation des accents).

## Où ça compte vraiment dans Cinemap

1. **Réseau** (le plus gros poste) : un seul `reqwest::Client` réutilisé (pool de connexions), concurrence limitée (~3 req/s), ne pas retélécharger ce qui n'a pas changé, s'arrêter tout de suite sur 403/429.
2. **SQLite** : écrire dans **une transaction groupée** (1 000 `INSERT` dans une transaction, c'est des centaines de fois plus rapide que 1 000 transactions). Index adaptés aux requêtes de l'API. Requêtes préparées (sqlx le fait).
3. **Parsing** (milliers de cinémas, dizaines de milliers de séances chaque nuit) : sélecteurs créés une fois, emprunter le texte du HTML plutôt que tout copier, ne garder que les champs utiles.
4. **API** : filtrer en SQL (bounding box) plutôt que charger toute la table pour filtrer en Rust, réponses gzip, pas de `SELECT *`.
5. **Le service lui-même** : un binaire axum tourne avec quelques Mo de RAM. Le profil release peut réduire encore la taille du binaire (`strip = true`, `lto = true` dans `[profile.release]` de `Cargo.toml`), à mesurer.

## Ce qui ne vaut pas la peine

- Micro-optimiser un code exécuté une fois par nuit sur 101 lignes.
- Sacrifier la lisibilité pour un gain qu'on n'a pas mesuré.
- `unsafe` « pour aller plus vite » : jamais sans une mesure qui le justifie, et pas dans ce projet.
