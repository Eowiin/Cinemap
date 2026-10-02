use scraper::{Html, Selector};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::cinemas::departments::Department;

#[derive(Debug, Deserialize)]
pub struct Cinema {
    id: String,
    name: String,
    address: Option<String>,
}

pub async fn get_cinemas_from_department(department: Department) -> anyhow::Result<()> {
    let url = format!("https://www.allocine.fr/salle/cinema/departement-{}", department.allocine_code.unwrap());

    debug!("Retrieving cinemas from {}", department.nom);
    let result = reqwest::get(url).await?.text().await?;

    // result.
    Ok(())
}

pub fn parse_department_page(html: &str) ->  Vec<Cinema> {
    let document = Html::parse_document(html);
    let mut cinemas: Vec<Cinema> = Vec::new();
    let card_selector = Selector::parse(".theater-card").expect("Sélecteur CSS invalide");
    let span_selector = Selector::parse("span[data-theater]").expect("Sélecteur CSS invalide");
    let address_selector = Selector::parse("address").expect("Sélecteur CSS invalide");

    for card in document.select(&card_selector) {
        let Some(data) = card
            .select(&span_selector)
            .next()
            .and_then(|el| el.value().attr("data-theater"))
        else {
            warn!("Carte cinéma sans attribut data-theater, ignorée");
            continue;
        };

        let mut cinema = match serde_json::from_str::<Cinema>(data) {
            Ok(cinema) => cinema,
            Err(e) => {
                warn!("data-theater invalide ({e}) : {data}");
                continue;
            }
        };

        // Adresse du cinéma
        cinema.address = card
            .select(&address_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        cinemas.push(cinema);
    }
    cinemas
}

pub fn page_count(html: &str) -> u32 {
    let document = Html::parse_document(html);
    let page_selector = Selector::parse("a[href*='?page=']")
        .expect("Sélecteur CSS invalide");
    document.select(&page_selector)
        .filter_map(|el| el.value().attr("href"))
        .filter_map(|href| href.rsplit_once('=').and_then(|(_, n)| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    const PAGE: &str = include_str!("../../tests/fixtures/departement-83093-p1.html");

    #[test]
    fn test_parse_department_page() {
        let cinemas = parse_department_page(PAGE);

        assert_eq!(cinemas.len(), 50, "Testing cinemas scrapping from AlloCine html page");

        let cinema = cinemas
            .iter()
            .find(|c| c.id == "C0159")
            .expect("Cinema C0159 not found");

        assert_eq!(cinema.name, "UGC Ciné Cité Les Halles");
        assert_eq!(
            cinema.address.as_deref(),
            Some("7 Place de la Rotonde 75001 Paris")
        );
    }

    #[test]
    fn test_get_page_count() {
        assert_eq!(page_count(PAGE), 15);
    }

    #[test]
    fn test_page_count_without_pagination() {
        let html = "<html><body></body></html>";

        assert_eq!(page_count(html), 1);
    }
}
