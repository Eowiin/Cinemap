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
    pub address: Option<String>,
}

pub async fn get_cinemas_from_department(
    department: &Department,
    client: &Client,
) -> anyhow::Result<Vec<Cinema>> {
    let path = department
        .allocine_path
        .as_deref()
        .context("Chemin AlloCiné manquant pour cette source")?;
    let mut cinemas: Vec<Cinema> = Vec::new();
    let mut page = 1;
    let mut max_page = 1;

    debug!("Récupération des cinémas : {}", &department.nom);
    while page <= max_page {
        let url = listing_url(path, page);
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

fn listing_url(path: &str, page: u32) -> String {
    format!("https://www.allocine.fr/salle/cinema/{path}/?page={page}")
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
    fetch_with_retries(
        client,
        url,
        &[
            Duration::from_secs(5),
            Duration::from_secs(15),
            Duration::from_secs(45),
        ],
    )
    .await
}

async fn fetch_with_retries(
    client: &Client,
    url: &str,
    delays: &[Duration],
) -> anyhow::Result<String> {
    let mut attempt = 0;
    loop {
        match fetch_once(client, url).await {
            Ok(html) => return Ok(html),
            Err(error) => {
                if !is_retryable(&error) || attempt == delays.len() {
                    return Err(error).with_context(|| {
                        format!("Échec de {url} après {} tentative(s)", attempt + 1)
                    });
                }
                let delay = delays[attempt];
                warn!(
                    url,
                    error = %error,
                    next_attempt = attempt + 2,
                    delay_secs = delay.as_secs(),
                    "Erreur réseau passagère, nouvel essai prévu"
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

fn is_retryable(error: &reqwest::Error) -> bool {
    if let Some(status) = error.status() {
        return status.is_server_error();
    }

    if error.is_timeout() || error.is_connect() {
        return true;
    }

    // Une coupure peut être enveloppée dans une erreur de requête ou de
    // décodage. Seule sa cause d'E/S justifie alors un nouvel essai.
    let mut cause = std::error::Error::source(error);
    while let Some(source) = cause {
        if let Some(io_error) = source.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;
            return matches!(
                io_error.kind(),
                ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::TimedOut
            );
        }
        cause = source.source();
    }
    false
}

async fn fetch_once(client: &Client, url: &str) -> Result<String, reqwest::Error> {
    client
        .get(url)
        .header(REFERER, "https://www.allocine.fr/")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn mock_server(
        responses: Vec<String>,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        let server = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let bytes = stream.read(&mut buffer).await.unwrap();
                    assert_ne!(bytes, 0);
                    request.extend_from_slice(&buffer[..bytes]);
                }
                assert!(
                    String::from_utf8_lossy(&request)
                        .to_lowercase()
                        .contains("referer: https://www.allocine.fr/")
                );
                requests.fetch_add(1, Ordering::SeqCst);
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (url, count, server)
    }

    fn response(status: u16) -> String {
        format!("HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
    }

    fn test_client() -> Client {
        Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn retries_server_errors_then_returns_body() {
        let (url, count, server) =
            mock_server(vec![response(500), response(503), response(200)]).await;
        let body = fetch_with_retries(&test_client(), &url, &[Duration::ZERO; 3])
            .await
            .unwrap();
        assert_eq!(body, "ok");
        assert_eq!(count.load(Ordering::SeqCst), 3);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn refuses_to_retry_client_errors() {
        for status in [403, 404, 429] {
            let (url, count, server) = mock_server(vec![response(status), response(200)]).await;
            let error = fetch_with_retries(&test_client(), &url, &[Duration::ZERO; 3])
                .await
                .unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<reqwest::Error>()
                    .unwrap()
                    .status()
                    .unwrap()
                    .as_u16(),
                status
            );
            assert_eq!(count.load(Ordering::SeqCst), 1);
            server.abort();
        }
    }

    #[tokio::test]
    async fn returns_last_error_after_four_failed_attempts() {
        let (url, count, server) = mock_server(vec![response(503); 4]).await;
        let error = fetch_with_retries(&test_client(), &url, &[Duration::ZERO; 3])
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<reqwest::Error>().unwrap().status(),
            Some(reqwest::StatusCode::SERVICE_UNAVAILABLE)
        );
        assert!(error.to_string().contains("4 tentative(s)"));
        assert_eq!(count.load(Ordering::SeqCst), 4);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn retries_interrupted_response_body() {
        let truncated =
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial".to_owned();
        let (url, count, server) = mock_server(vec![truncated, response(200)]).await;
        assert_eq!(
            fetch_with_retries(&test_client(), &url, &[Duration::ZERO; 3])
                .await
                .unwrap(),
            "ok"
        );
        assert_eq!(count.load(Ordering::SeqCst), 2);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn malformed_response_is_not_retried() {
        let malformed = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\ninvalid-size\r\nbody\r\n".to_owned();
        let (url, count, server) = mock_server(vec![malformed, response(200)]).await;
        let error = fetch_with_retries(&test_client(), &url, &[Duration::ZERO; 3])
            .await
            .unwrap_err();
        assert!(!is_retryable(
            error.downcast_ref::<reqwest::Error>().unwrap()
        ));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn timeout_and_connection_failure_are_retryable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let client = Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .timeout(Duration::from_millis(30))
            .build()
            .unwrap();
        let error = fetch_once(&client, &url).await.unwrap_err();
        assert!(error.is_timeout());
        assert!(is_retryable(&error));
        drop(listener);
        let error = fetch_once(&test_client(), &url).await.unwrap_err();
        assert!(error.is_connect());
        assert!(is_retryable(&error));
        let error = fetch_once(&client, "://invalid").await.unwrap_err();
        assert!(!is_retryable(&error));
    }

    const PAGE: &str = include_str!("../../tests/fixtures/departement-83093-p1.html");
    const PARIS_PAGE: &str = include_str!("../../tests/fixtures/ville-115755-p1.html");

    #[test]
    fn parses_paris_city_page_with_the_same_parser() {
        let cinemas = parse_department_page(PARIS_PAGE);
        assert_eq!(cinemas.len(), 20);
        assert!(
            cinemas
                .iter()
                .all(|cinema| !cinema.id.is_empty() && !cinema.name.is_empty())
        );
        assert!(cinemas.iter().any(|cinema| cinema.id == "C0159"));
        assert!(page_count(PARIS_PAGE) >= 6);
    }

    #[test]
    fn listing_urls_use_configured_paths_and_trailing_slash() {
        assert_eq!(
            listing_url("ville-115755", 2),
            "https://www.allocine.fr/salle/cinema/ville-115755/?page=2"
        );
        assert_eq!(
            listing_url("departement-83169", 1),
            "https://www.allocine.fr/salle/cinema/departement-83169/?page=1"
        );
    }

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
            .expect("Cinéma C0159 introuvable");

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
