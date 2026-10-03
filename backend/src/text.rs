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
    fn test_department_from_empty() {
        assert_eq!(department_from_insee(""), None);
    }
}
