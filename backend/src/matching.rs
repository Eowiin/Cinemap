//! Croisement de noms de cinémas entre deux sources : CNC (`cinemas/cnc.rs`) et
//! cartes illimitées (`cards/`). Les deux trouvent d'abord des candidats proches
//! (même commune, même code postal, distance), puis départagent par le nom.

use std::{collections::HashSet, hash::Hash};

use strsim::jaro_winkler;

use crate::text::normalize;

/// Mots qui diffèrent souvent d'une source à l'autre sans changer l'établissement.
/// Les marques (« ugc », « pathe », « mk2 », « gaumont ») n'y sont **pas** : elles
/// distinguent « UGC Montparnasse » de « Pathé Montparnasse ».
const GENERIC: &[&str] = &[
    "cinema", "cinemas", "cine", "salle", "mega", "le", "la", "les", "l", "de", "du", "des", "d",
];

/// Mots normalisés (minuscules, sans accents ni ponctuation), « st » et « ste »
/// développés, nombres de deux à dix en chiffres.
pub fn words(text: &str) -> Vec<String> {
    normalize(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| match word {
            "st" => "saint",
            "ste" => "sainte",
            // « Les 7 Parnassiens » / « Sept Parnassiens ». Pas « un » : c'est aussi un article.
            "deux" => "2",
            "trois" => "3",
            "quatre" => "4",
            "cinq" => "5",
            "six" => "6",
            "sept" => "7",
            "huit" => "8",
            "neuf" => "9",
            "dix" => "10",
            other => other,
        })
        .map(str::to_owned)
        .collect()
}

/// Nom normalisé sans ponctuation ni mots génériques (« cinéma », « le »…).
/// Si le nom ne contient que des mots génériques (« Cinéma »), on les garde.
pub fn comparable_name(name: &str) -> String {
    let words = words(name);
    let specific: Vec<&str> = words
        .iter()
        .map(String::as_str)
        .filter(|word| !GENERIC.contains(word))
        .collect();
    if specific.is_empty() {
        words.join(" ")
    } else {
        specific.join(" ")
    }
}

/// Le meilleur de deux mesures : `jaro_winkler` tolère les fautes et abréviations
/// (« st paul » / « saint paul ») ; l'inclusion des mots couvre les noms complétés
/// d'un côté (« comoedia » / « sete comoedia », « champo » / « champo espace jacques tati »).
pub fn similarity(a: &str, b: &str) -> f64 {
    jaro_winkler(a, b).max(word_containment(a, b))
}

/// Part des mots du nom le plus court présents dans l'autre nom.
pub fn word_containment(a: &str, b: &str) -> f64 {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let long_words: Vec<&str> = long.split(' ').collect();
    let short_words: Vec<&str> = short.split(' ').filter(|w| !w.is_empty()).collect();
    if short_words.is_empty() {
        return 0.0;
    }
    let found = short_words
        .iter()
        .filter(|w| long_words.contains(w))
        .count();
    found as f64 / short_words.len() as f64
}

/// Similarité sur les seuls mots qui **diffèrent** entre les deux noms.
///
/// `jaro_winkler` donne un bonus au préfixe commun : « ugc montparnasse » et
/// « ugc rotonde » obtiennent 0,854, « ugc cite noisy grand » et « ugc cite rosny »
/// 0,926. Sans les mots communs, on compare « montparnasse » à « rotonde ».
/// Si tous les mots d'un nom sont dans l'autre, le nom est inclus : 1.
pub fn distinctive_similarity(a: &str, b: &str) -> f64 {
    let a_words: Vec<&str> = a.split(' ').filter(|w| !w.is_empty()).collect();
    let b_words: Vec<&str> = b.split(' ').filter(|w| !w.is_empty()).collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let a_rest: Vec<&str> = a_words
        .iter()
        .copied()
        .filter(|w| !b_words.contains(w))
        .collect();
    let b_rest: Vec<&str> = b_words
        .iter()
        .copied()
        .filter(|w| !a_words.contains(w))
        .collect();
    if a_rest.is_empty() || b_rest.is_empty() {
        return 1.0;
    }
    jaro_winkler(&a_rest.join(" "), &b_rest.join(" "))
}

/// Part des mots communs aux deux noms (indice de Jaccard). Sert à départager deux
/// noms inclus à égalité : « langon » est plus proche de « grand ecran langon » que de
/// « grand ecran langon rio centre ville ».
pub fn word_overlap(a: &str, b: &str) -> f64 {
    let a: HashSet<&str> = a.split(' ').filter(|w| !w.is_empty()).collect();
    let b: HashSet<&str> = b.split(' ').filter(|w| !w.is_empty()).collect();
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / union as f64
}

/// Paire candidate : `a` et `b` sont des clés (identifiant, indice dans une liste).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scored<A, B> {
    pub a: A,
    pub b: B,
    pub score: f64,
    /// Départage les égalités de `score` (le nom le plus proche lettre à lettre).
    pub closeness: f64,
}

/// Attribue chaque `a` à au plus un `b`, et inversement. Les paires sont traitées de
/// la meilleure à la moins bonne : si deux `a` visent le même `b`, le plus proche gagne.
/// Tri stable : à égalité parfaite, l'ordre d'entrée décide, le résultat est reproductible.
pub fn assign_best<A, B>(mut pairs: Vec<Scored<A, B>>) -> Vec<Scored<A, B>>
where
    A: Copy + Eq + Hash,
    B: Copy + Eq + Hash,
{
    pairs.sort_by(|x, y| {
        y.score
            .total_cmp(&x.score)
            .then(y.closeness.total_cmp(&x.closeness))
    });
    let mut used_a = HashSet::new();
    let mut used_b = HashSet::new();
    pairs.retain(|pair| {
        let free = !used_a.contains(&pair.a) && !used_b.contains(&pair.b);
        if free {
            used_a.insert(pair.a);
            used_b.insert(pair.b);
        }
        free
    });
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinctive_similarity_ignores_shared_brand_words() {
        assert!(distinctive_similarity("ugc montparnasse", "ugc rotonde") < 0.85);
        assert!(distinctive_similarity("ugc cite noisy grand", "ugc cite rosny") < 0.85);
        assert!(distinctive_similarity("ugc montparnasse", "pathe montparnasse") < 0.85);
        assert_eq!(distinctive_similarity("ugc cite", "ugc cite lille"), 1.0);
        assert_eq!(
            distinctive_similarity("ugc normandie", "ugc normandie"),
            1.0
        );
        assert!(distinctive_similarity("ugc normandie", "ugc normandy") > 0.85);
        assert_eq!(distinctive_similarity("", "ugc"), 0.0);
    }

    #[test]
    fn assign_best_gives_each_side_once_best_first() {
        let pair = |a, b, score| Scored {
            a,
            b,
            score,
            closeness: 0.0,
        };
        let assigned = assign_best(vec![
            pair("A", 1, 0.9),
            pair("B", 1, 1.0),
            pair("A", 2, 0.86),
            pair("B", 2, 0.95),
        ]);
        let keys: Vec<_> = assigned.iter().map(|p| (p.a, p.b)).collect();
        assert_eq!(keys, [("B", 1), ("A", 2)]);
    }
}
