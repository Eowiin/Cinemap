use std::time::Duration;

use anyhow::Context;
use reqwest::{Client, header::REFERER};
use scraper::{Html, Selector};
use serde::Deserialize;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::cinemas::departments::Department;

#[derive(Deserialize)]
pub struct Cinema {
    pub id: String,
    pub name: String,
    pub address: Option<String>
}

pub async fn get_cinemas_from_department(
    department: &Department,
    client: &Client,
) -> anyhow::Result<Vec<Cinema>> {
    let base_url = format!("https://www.allocine.fr/salle/cinema/departement-{}", department.allocine_code.as_ref().unwrap());
    let mut cinemas: Vec<Cinema> = Vec::new();
    let mut page = 1;
    let mut max_page = 1;

    debug!("Retrieving cinemas from {}", &department.nom);
    while page <= max_page {
        let url = format!("{base_url}?page={page}");
        let html = fetch(client, &url).await?;

        max_page = max_page.max(page_count(&html));
        cinemas.append(&mut parse_department_page(&html));
        page += 1;

        if page <= max_page {
            sleep(Duration::from_millis(3000)).await;
        }
    }
    Ok(cinemas)
}

pub fn parse_department_page(html: &str) -> Vec<Cinema> {
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

fn page_count(html: &str) -> u32 {
    let document = Html::parse_document(html);
    let page_selector = Selector::parse("a[href*='?page=']").expect("Sélecteur CSS invalide");
    document
        .select(&page_selector)
        .filter_map(|el| el.value().attr("href"))
        .filter_map(|href| {
            href.rsplit_once('=')
                .and_then(|(_, n)| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(1)
}

async fn fetch(client: &Client, url: &str) -> anyhow::Result<String> {
    client
        .get(url)
        .header(REFERER, "https://www.allocine.fr/")
        .send()
        .await
        .with_context(|| format!("Failed to send request to {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {url}"))?
        .text()
        .await
        .with_context(|| format!("Failed to read response from {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    const PAGE: &str = include_str!("../../tests/fixtures/departement-83093-p1.html");

    #[test]
    fn test_parse_department_page() {
        let cinemas = parse_department_page(PAGE);

        assert_eq!(
            cinemas.len(),
            50,
            "Testing cinemas scrapping from AlloCine html page"
        );

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
