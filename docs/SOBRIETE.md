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
