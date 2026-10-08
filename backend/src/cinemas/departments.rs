use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct Department {
    pub code_insee: String,
    pub nom: String,
    pub allocine_path: Option<String>,
}

pub fn get_departments() -> impl Iterator<Item = Department> {
    let file = include_str!("../../../docs/data/departements.csv");
    let reader = csv::Reader::from_reader(file.as_bytes());

    reader
        .into_deserialize::<Department>()
        .filter_map(|x| match x {
            Ok(x) => {
                if x.allocine_path.is_none() {
                    warn!(
                        "Chemin AlloCiné manquant pour le département {} ({}), ignoré",
                        x.nom, x.code_insee
                    );
                    return None;
                }
                Some(x)
            }
            Err(e) => {
                warn!("Ligne du CSV des départements invalide : {e}");
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_departments() {
        let sources: Vec<Department> = get_departments().collect();
        let mut codes: Vec<&str> = sources.iter().map(|d| d.code_insee.as_str()).collect();
        codes.sort_unstable();
        codes.dedup();

        // 100 départements (sans Mayotte), plus les pages ville de Lyon et Marseille.
        assert_eq!(sources.len(), 102);
        assert_eq!(codes.len(), 100);
    }

    #[test]
    fn lyon_and_marseille_add_their_city_listing() {
        let paths = |code: &str| -> Vec<String> {
            get_departments()
                .filter(|department| department.code_insee == code)
                .filter_map(|department| department.allocine_path)
                .collect()
        };
        assert_eq!(paths("69"), ["departement-83196", "ville-113315"]);
        assert_eq!(paths("13"), ["departement-83188", "ville-87914"]);
    }

    #[test]
    fn paris_uses_the_city_listing() {
        let paris = get_departments()
            .find(|department| department.code_insee == "75")
            .unwrap();
        assert_eq!(paris.allocine_path.as_deref(), Some("ville-115755"));
    }

    #[test]
    fn source_paths_are_valid_listing_segments() {
        for department in get_departments() {
            let path = department.allocine_path.as_deref().unwrap();
            let id = path
                .strip_prefix("departement-")
                .or_else(|| path.strip_prefix("ville-"))
                .expect("Chemin de département ou de ville attendu");
            assert!(!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
        }
    }
}
