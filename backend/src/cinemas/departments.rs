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
                        "Error: no allocine_path for department {} ({})",
                        x.nom, x.code_insee
                    );
                    return None;
                }
                Some(x)
            }
            Err(e) => {
                warn!("Error: {e}");
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_departments() {
        let departments = get_departments().count();

        assert_eq!(
            departments, 100,
            "Testing departments retrieval from department.csv"
        );
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
