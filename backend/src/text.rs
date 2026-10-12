use deunicode::deunicode;

pub fn normalize(text: &str) -> String {
    deunicode(text).to_lowercase()
}

pub fn department_from_insee(code: &str) -> Option<&str> {
    if code.len() != 5 || !code.is_ascii() {
        return None;
    }

    let prefix = if code.starts_with("97") { 3 } else { 2 };
    code.get(..prefix)
}

/// Longueur maximale d'un slug : au-delà, on coupe au dernier mot entier.
const SLUG_MAX_LEN: usize = 60;

/// Segment lisible d'URL : « L'Odyssée » → "l-odyssee". Vide si rien de latin à garder.
/// Le front a la même règle (`slugify` dans `routes.ts`) : les liens générés là-bas
/// tombent directement sur l'URL canonique, sans redirection.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for c in deunicode(text).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    if slug.len() > SLUG_MAX_LEN {
        // Que de l'ASCII : couper à un octet quelconque est sûr.
        let cut = slug[..=SLUG_MAX_LEN].rfind('-').unwrap_or(SLUG_MAX_LEN);
        slug.truncate(cut);
    }
    let len = slug.trim_end_matches('-').len();
    slug.truncate(len);
    slug
}

/// Slug d'un cinéma : son nom, suivi de la ville si le nom ne la contient pas déjà
/// (« Pathé Bellecour », Lyon → "pathe-bellecour-lyon" ; « Pathé Lyon » → "pathe-lyon").
pub fn cinema_slug(name: &str, city: Option<&str>) -> String {
    let name = slugify(name);
    match city.map(slugify).filter(|city| !city.is_empty()) {
        Some(city) if !format!("-{name}-").contains(&format!("-{city}-")) => {
            if name.is_empty() {
                city
            } else {
                format!("{name}-{city}")
            }
        }
        _ => name,
    }
}

/// Ville affichée, avec l'arrondissement à Paris, Lyon et Marseille (75004 → « Paris 4e »).
pub fn locality(city: &str, postal_code: Option<&str>) -> String {
    let prefix = match city {
        "Paris" => "75",
        "Lyon" => "69",
        "Marseille" => "13",
        _ => return city.to_owned(),
    };
    let max = match city {
        "Paris" => 20,
        "Lyon" => 9,
        _ => 16,
    };
    // 75116 : seconde moitié du 16e, même arrondissement.
    let district = postal_code
        .filter(|code| code.len() == 5 && code.starts_with(prefix))
        .and_then(|code| code[2..].parse::<u32>().ok())
        .map(|n| if n == 116 { 16 } else { n })
        .filter(|n| (1..=max).contains(n));
    match district {
        Some(1) => format!("{city} 1er"),
        Some(n) => format!("{city} {n}e"),
        None => city.to_owned(),
    }
}

/// Byte offset of the last standalone block of five ASCII digits.
pub fn get_postal_code(address: &str) -> Option<usize> {
    let bytes = address.as_bytes();
    bytes.windows(5).enumerate().rev().find_map(|(i, window)| {
        (window.iter().all(u8::is_ascii_digit)
            && (i == 0 || !bytes[i - 1].is_ascii_digit())
            && bytes.get(i + 5).is_none_or(|b| !b.is_ascii_digit()))
        .then_some(i)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_cinema_name() {
        assert_eq!(normalize("Évry Cinéma"), "evry cinema");
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn test_department_from_insee_code() {
        assert_eq!(department_from_insee("75101"), Some("75"));
        assert_eq!(department_from_insee("97105"), Some("971"));
        assert_eq!(department_from_insee("2A004"), Some("2A"));
        assert_eq!(department_from_insee("2B033"), Some("2B"));
    }

    #[test]
    fn slugify_keeps_latin_letters_and_digits() {
        assert_eq!(slugify("L'Odyssée de Pi"), "l-odyssee-de-pi");
        assert_eq!(slugify("  Cœur & Âme : 2 ! "), "coeur-ame-2");
        assert_eq!(slugify("..."), "");
    }

    #[test]
    fn slugify_cuts_long_titles_on_a_word() {
        let slug = slugify(&"cinema ".repeat(20));
        assert!(slug.len() <= SLUG_MAX_LEN, "{slug}");
        assert!(slug.ends_with("cinema"), "{slug}");
    }

    #[test]
    fn cinema_slug_adds_the_city_once() {
        assert_eq!(
            cinema_slug("Luminor Hôtel de Ville", Some("Paris")),
            "luminor-hotel-de-ville-paris"
        );
        assert_eq!(cinema_slug("Pathé Lyon", Some("Lyon")), "pathe-lyon");
        // « Lyon » n'est pas un mot de « Lyonnais » : la ville est ajoutée.
        assert_eq!(cinema_slug("Le Lyonnais", Some("Lyon")), "le-lyonnais-lyon");
        assert_eq!(cinema_slug("Le Rex", None), "le-rex");
    }

    #[test]
    fn locality_names_the_district() {
        assert_eq!(locality("Paris", Some("75004")), "Paris 4e");
        assert_eq!(locality("Paris", Some("75001")), "Paris 1er");
        assert_eq!(locality("Paris", Some("75116")), "Paris 16e");
        assert_eq!(locality("Lyon", Some("69002")), "Lyon 2e");
        assert_eq!(locality("Marseille", Some("13008")), "Marseille 8e");
        assert_eq!(locality("Paris", None), "Paris");
        assert_eq!(locality("Lyon", Some("69100")), "Lyon");
        assert_eq!(locality("Rennes", Some("35000")), "Rennes");
    }

    #[test]
    fn test_department_from_empty() {
        assert_eq!(department_from_insee(""), None);
    }
}
