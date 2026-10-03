use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct Department {
    pub code_insee: String,
    pub nom: String,
    pub allocine_code: Option<String>,
}

pub fn get_departments() -> impl Iterator<Item = Department> {
    let file = include_str!("../../../docs/data/departements.csv");
    let reader = csv::Reader::from_reader(file.as_bytes());

    reader
        .into_deserialize::<Department>()
        .filter_map(|x| match x {
            Ok(x) => {
                if x.allocine_code.is_none() {
                    warn!(
                        "Error: no allocine_code for department {} ({})",
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
}
