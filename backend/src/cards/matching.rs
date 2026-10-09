//! Croisement d'une liste de carte avec nos cinémas. Fonctions pures, testées sans base.

use std::collections::{HashMap, HashSet};

use super::CardCinema;
use crate::matching::{
    Scored, assign_best, comparable_name, distinctive_similarity, word_overlap, words,
};

/// Similarité minimale des noms (sur les mots distinctifs, voir `distinctive_similarity`).
pub const MIN_SIMILARITY: f64 = 0.85;
/// Pathé, première passe : à moins de 500 m, le nom ne fait que départager, avec un
/// seuil bas (un multiplexe voisin ne doit quand même pas prendre la carte).
pub const NEAR_M: f64 = 500.0;
pub const MIN_SIMILARITY_NEAR: f64 = 0.7;
/// Pathé, seconde passe : notre géocodage place mal les multiplexes de centres
/// commerciaux (600 m à 3 km d'écart, score de géocodage ~0,5). Jusqu'à 5 km, avec le
/// seuil normal sur le nom.
pub const FAR_M: f64 = 5_000.0;

/// Un cinéma de notre base, masqué ou non : un cinéma qui réapparaît garde sa carte.
#[derive(Debug)]
pub struct CardTarget {
    pub id: String,
    pub name: String,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub department: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[derive(Debug)]
pub struct CardMatch<'a> {
    pub cinema_id: &'a str,
    pub entry: &'a CardCinema,
    pub score: f64,
    /// Comment le candidat a été trouvé, pour le log de relecture.
    pub how: How,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum How {
    PostalCode,
    /// Repli : même département et même ville (code CEDEX, ou code postal différent).
    City,
    Distance(f64),
}

/// UGC : première passe au même code postal ; seconde passe, pour les entrées et les
/// cinémas restants, dans le même département et la même ville (code CEDEX, ou code
/// postal différent d'AlloCiné : 75005 / 75006 pour le MK2 Odéon, 59000 / 59800 à Lille).
pub fn match_by_postal_code<'a>(
    cinemas: &'a [CardTarget],
    entries: &'a [CardCinema],
) -> Vec<CardMatch<'a>> {
    // Indices dans `cinemas`, regroupés par code postal et par (département, ville).
    let mut by_postal_code: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_city: HashMap<(&str, String), Vec<usize>> = HashMap::new();
    for (index, cinema) in cinemas.iter().enumerate() {
        if let Some(postal_code) = &cinema.postal_code {
            by_postal_code.entry(postal_code).or_default().push(index);
        }
        if let (Some(department), Some(city)) = (&cinema.department, &cinema.city) {
            by_city
                .entry((department, city_key(city)))
                .or_default()
                .push(index);
        }
    }

    let mut done = Vec::new();
    pass(cinemas, entries, &mut done, MIN_SIMILARITY, |entry| {
        let candidates = by_postal_code.get(entry.postal_code.as_str());
        candidates
            .into_iter()
            .flatten()
            .map(|&c| (c, How::PostalCode))
            .collect()
    });
    pass(cinemas, entries, &mut done, MIN_SIMILARITY, |entry| {
        let city = city_key(&entry.city);
        department_variants(entry_department(&entry.postal_code))
            .into_iter()
            .filter_map(|department| by_city.get(&(department, city.clone())))
            .flatten()
            .map(|&c| (c, How::City))
            .collect()
    });
    into_matches(done, cinemas, entries)
}

/// Pathé : par distance (les codes postaux Pathé sont souvent des CEDEX), puis le nom.
/// Deux passes : moins de `NEAR_M`, puis jusqu'à `FAR_M` avec un seuil de nom plus haut.
/// Une entrée sans position n'est pas croisée.
pub fn match_by_distance<'a>(
    cinemas: &'a [CardTarget],
    entries: &'a [CardCinema],
) -> Vec<CardMatch<'a>> {
    let within = |radius: f64| {
        move |entry: &CardCinema| -> Vec<(usize, How)> {
            let Some(position) = entry.position else {
                return Vec::new();
            };
            cinemas
                .iter()
                .enumerate()
                .filter_map(|(index, cinema)| {
                    let distance = haversine_m(position, (cinema.lat?, cinema.lng?));
                    (distance <= radius).then_some((index, How::Distance(distance)))
                })
                .collect()
        }
    };
    let mut done = Vec::new();
    pass(
        cinemas,
        entries,
        &mut done,
        MIN_SIMILARITY_NEAR,
        within(NEAR_M),
    );
    pass(cinemas, entries, &mut done, MIN_SIMILARITY, within(FAR_M));
    into_matches(done, cinemas, entries)
}

/// Clés de `assign_best` : indice du cinéma dans `cinemas`, indice de l'entrée.
type Pair = (Scored<usize, usize>, How);

/// Une passe : pour chaque entrée encore libre, ses candidats (`candidates`) encore
/// libres et assez ressemblants, attribués du meilleur au moins bon et ajoutés à `done`.
fn pass(
    cinemas: &[CardTarget],
    entries: &[CardCinema],
    done: &mut Vec<Pair>,
    min: f64,
    candidates: impl Fn(&CardCinema) -> Vec<(usize, How)>,
) {
    let used_cinemas: HashSet<usize> = done.iter().map(|(pair, _)| pair.a).collect();
    let used_entries: HashSet<usize> = done.iter().map(|(pair, _)| pair.b).collect();
    let mut pairs = Vec::new();
    let mut how = HashMap::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        if used_entries.contains(&entry_index) {
            continue;
        }
        for (cinema_index, found) in candidates(entry) {
            if used_cinemas.contains(&cinema_index) {
                continue;
            }
            if let Some(pair) = score(&cinemas[cinema_index], cinema_index, entry, entry_index)
                && pair.score >= min
            {
                how.insert((cinema_index, entry_index), found);
                pairs.push(pair);
            }
        }
    }
    done.extend(
        assign_best(pairs)
            .into_iter()
            .map(|pair| (pair, how[&(pair.a, pair.b)])),
    );
}

/// Le meilleur de deux comparaisons sur les mots distinctifs :
/// - sans les mots de la ville (« Limoges Ester » / « GRAND ECRAN ESTER ») ;
/// - noms entiers, pour un cinéma qui ne porte que le nom de sa ville
///   (« Arcachon » / « GRAND ÉCRAN ARCACHON »).
fn score(
    cinema: &CardTarget,
    cinema_index: usize,
    entry: &CardCinema,
    entry_index: usize,
) -> Option<Scored<usize, usize>> {
    let ours = card_name(&cinema.name, cinema.city.as_deref());
    let theirs = card_name(&entry.name, Some(&entry.city));
    let (ours_full, theirs_full) = (comparable_name(&cinema.name), comparable_name(&entry.name));
    let score = distinctive_similarity(&ours, &theirs)
        .max(distinctive_similarity(&ours_full, &theirs_full));
    (score > 0.0).then(|| Scored {
        a: cinema_index,
        b: entry_index,
        score,
        closeness: word_overlap(&ours_full, &theirs_full),
    })
}

fn into_matches<'a>(
    done: Vec<Pair>,
    cinemas: &'a [CardTarget],
    entries: &'a [CardCinema],
) -> Vec<CardMatch<'a>> {
    done.into_iter()
        .map(|(pair, how)| CardMatch {
            cinema_id: &cinemas[pair.a].id,
            entry: &entries[pair.b],
            score: pair.score,
            how,
        })
        .collect()
}

/// Nom comparable, sans les mots de la ville : AlloCiné écrit souvent « Limoges Ester »
/// ou « UGC Ciné Cité Lyon Part-Dieu », la carte « GRAND ECRAN ESTER ». Si le nom ne
/// contient que la ville (« Cinéma de Bastia »), on la garde.
pub fn card_name(name: &str, city: Option<&str>) -> String {
    let name = comparable_name(name);
    let city_words = city.map(words).unwrap_or_default();
    let rest: Vec<&str> = name
        .split(' ')
        .filter(|word| !city_words.iter().any(|c| c == word))
        .collect();
    if rest.is_empty() {
        name
    } else {
        rest.join(" ")
    }
}

/// Ville comparable : sans accents ni ponctuation, sans « CEDEX » ni numéro
/// (« NOISY-LE-GRAND CEDEX » → « noisy le grand »).
pub fn city_key(city: &str) -> String {
    words(city)
        .into_iter()
        .filter(|word| word != "cedex" && !word.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Département tel que l'écrit un code postal : 3 chiffres outre-mer (`974…`), sinon 2.
fn entry_department(postal_code: &str) -> &str {
    let len = if postal_code.starts_with("97") { 3 } else { 2 };
    postal_code.get(..len).unwrap_or(postal_code)
}

/// Codes de notre colonne `department` possibles pour un préfixe de code postal :
/// les codes postaux corses commencent par `20`, nos départements sont `2A` / `2B`.
fn department_variants(prefix: &str) -> Vec<&str> {
    if prefix == "20" {
        vec!["2A", "2B"]
    } else {
        vec![prefix]
    }
}

/// Distance en mètres entre deux points `(lat, lng)` en degrés (sphère de rayon moyen).
pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let d_lat = lat2 - lat1;
    let d_lng = (b.1 - a.1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cinema(id: &str, name: &str, postal_code: &str, city: &str) -> CardTarget {
        CardTarget {
            id: id.into(),
            name: name.into(),
            postal_code: Some(postal_code.into()),
            city: Some(city.into()),
            department: Some(
                match postal_code {
                    p if p.starts_with("20") => "2A",
                    p => &p[..2],
                }
                .into(),
            ),
            lat: None,
            lng: None,
        }
    }

    fn located(id: &str, name: &str, lat: f64, lng: f64) -> CardTarget {
        CardTarget {
            lat: Some(lat),
            lng: Some(lng),
            ..cinema(id, name, "75000", "Paris")
        }
    }

    fn entry(name: &str, postal_code: &str, city: &str) -> CardCinema {
        CardCinema {
            name: name.into(),
            postal_code: postal_code.into(),
            city: city.into(),
            position: None,
        }
    }

    fn pairs(matches: &[CardMatch]) -> Vec<(String, String)> {
        let mut pairs: Vec<_> = matches
            .iter()
            .map(|m| (m.cinema_id.to_owned(), m.entry.name.clone()))
            .collect();
        pairs.sort();
        pairs
    }

    #[test]
    fn matches_by_postal_code_and_name() {
        let cinemas = [
            cinema("C1", "UGC Ciné Cité Les Halles", "75001", "Paris"),
            cinema("C2", "Pathé Montparnasse", "75006", "Paris"),
            cinema("C3", "UGC Montparnasse", "75006", "Paris"),
            cinema("C4", "UGC Rotonde", "75006", "Paris"),
        ];
        let entries = [
            entry("UGC Ciné Cité Les Halles", "75001", "PARIS"),
            entry("UGC MONTPARNASSE", "75006", "PARIS"),
            entry("UGC ROTONDE", "75006", "PARIS"),
        ];

        let matches = match_by_postal_code(&cinemas, &entries);

        assert_eq!(
            pairs(&matches),
            [
                ("C1".into(), "UGC Ciné Cité Les Halles".into()),
                ("C3".into(), "UGC MONTPARNASSE".into()),
                ("C4".into(), "UGC ROTONDE".into()),
            ]
        );
        assert!(matches.iter().all(|m| m.how == How::PostalCode));
    }

    #[test]
    fn a_missing_cinema_does_not_steal_a_neighbour() {
        // Avec jaro_winkler sur le nom entier, « ugc montparnasse » / « ugc rotonde » = 0,854.
        let cinemas = [cinema("C4", "UGC Rotonde", "75006", "Paris")];
        let entries = [entry("UGC MONTPARNASSE", "75006", "PARIS")];

        assert!(match_by_postal_code(&cinemas, &entries).is_empty());
    }

    #[test]
    fn cedex_falls_back_to_the_same_city() {
        let cinemas = [
            cinema(
                "B0114",
                "UGC Ciné Cité Noisy-le-Grand",
                "93160",
                "Noisy-le-Grand",
            ),
            cinema("C9", "UGC Ciné Cité Rosny", "93110", "Rosny-sous-Bois"),
        ];
        let entries = [entry(
            "UGC Ciné Cité Noisy-le-Grand",
            "93193",
            "NOISY-LE-GRAND CEDEX",
        )];

        let matches = match_by_postal_code(&cinemas, &entries);

        assert_eq!(
            pairs(&matches),
            [("B0114".into(), "UGC Ciné Cité Noisy-le-Grand".into())]
        );
        assert_eq!(matches[0].how, How::City);
    }

    #[test]
    fn city_words_are_ignored_in_names() {
        // AlloCiné : « Limoges Ester » (87000) ; UGC : « GRAND ECRAN ESTER » (87100).
        let cinemas = [
            cinema("P8001", "Limoges Ester", "87000", "Limoges"),
            cinema("P0170", "Limoges Centre", "87000", "Limoges"),
        ];
        let entries = [
            entry("GRAND ECRAN ESTER", "87100", "LIMOGES"),
            entry("HORIZON GRAND ECRAN", "87100", "LIMOGES"),
        ];

        let matches = match_by_postal_code(&cinemas, &entries);

        assert_eq!(
            pairs(&matches),
            [("P8001".into(), "GRAND ECRAN ESTER".into())]
        );
    }

    #[test]
    fn closest_entry_wins_when_both_contain_the_name() {
        // Un seul cinéma « Langon » chez AlloCiné (le multiplexe), deux entrées UGC.
        let cinemas = [cinema("W3321", "Langon", "33210", "Langon")];
        let entries = [
            entry("GRAND ECRAN LANGON RIO CENTRE-VILLE", "33210", "LANGON"),
            entry("GRAND ECRAN LANGON", "33210", "LANGON"),
        ];

        assert_eq!(
            pairs(&match_by_postal_code(&cinemas, &entries)),
            [("W3321".into(), "GRAND ECRAN LANGON".into())]
        );
    }

    #[test]
    fn corsican_postal_codes_find_2a_and_2b() {
        let cinemas = [cinema("P0927", "Cinéma Laetitia", "20000", "Ajaccio")];
        let entries = [entry("LAETITIA", "20090", "AJACCIO CEDEX")];

        assert_eq!(match_by_postal_code(&cinemas, &entries).len(), 1);
    }

    #[test]
    fn a_name_without_candidate_is_not_matched() {
        let cinemas = [cinema("C1", "Le Balzac", "75008", "Paris")];
        let entries = [entry("UGC NORMANDIE", "75008", "PARIS")];

        assert!(match_by_postal_code(&cinemas, &entries).is_empty());
    }

    #[test]
    fn matches_by_distance_then_name() {
        // La Géode et Pathé La Villette : même site.
        let cinemas = [
            located("C1", "Pathé La Villette", 48.8945, 2.3876),
            located("C2", "La Géode", 48.8955, 2.3880),
            located("C3", "Pathé Wepler", 48.8836, 2.3274),
        ];
        let entries = [
            CardCinema {
                position: Some((48.8946, 2.3877)),
                ..entry("Pathé La Villette", "75019", "Paris")
            },
            CardCinema {
                position: Some((48.8836, 2.3275)),
                ..entry("Pathé Wepler", "75018", "Paris")
            },
            // À plus de 500 m de tout : pas croisé, même avec le même nom.
            CardCinema {
                position: Some((48.80, 2.30)),
                ..entry("Pathé Wepler", "75018", "Paris")
            },
        ];

        let matches = match_by_distance(&cinemas, &entries);

        assert_eq!(
            pairs(&matches),
            [
                ("C1".into(), "Pathé La Villette".into()),
                ("C3".into(), "Pathé Wepler".into()),
            ]
        );
    }

    #[test]
    fn haversine_paris_lyon() {
        let distance = haversine_m((48.8566, 2.3522), (45.7640, 4.8357));
        assert!((distance - 391_500.0).abs() < 1_500.0, "{distance}");
        assert_eq!(haversine_m((48.0, 2.0), (48.0, 2.0)), 0.0);
    }

    #[test]
    fn city_key_drops_cedex_and_numbers() {
        assert_eq!(city_key("NOISY-LE-GRAND CEDEX"), "noisy le grand");
        assert_eq!(city_key("Paris Cedex 12"), "paris");
        assert_eq!(city_key("St-Étienne"), "saint etienne");
    }

    #[test]
    fn card_name_drops_the_city_unless_nothing_is_left() {
        assert_eq!(card_name("Limoges Ester", Some("Limoges")), "ester");
        assert_eq!(card_name("Cinéma de Bastia", Some("Bastia")), "bastia");
    }
}
