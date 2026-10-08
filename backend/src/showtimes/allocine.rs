use std::collections::BTreeMap;

use crate::client::{RETRY_DELAYS, fetch_with_retries_and_hooks};
use anyhow::{Context, bail};
use reqwest::{Client, header::ACCEPT};
use serde::Deserialize;

use super::{CircuitBreaker, SharedRateLimiter};

const BASE_URL: &str = "https://www.allocine.fr";

/// `#[serde(default)]` ne couvre que le champ **absent** ; AlloCiné envoie parfois `null`
/// à la place d'une liste (`"languages": null` vu sur C0020 le 2026-10-08). Avec ce
/// `deserialize_with`, `null` donne aussi la valeur par défaut (liste vide, struct vide).
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Response {
    pub(super) error: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    pub(super) next_date: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pagination: Pagination,
    #[serde(default, deserialize_with = "results_with_movie")]
    pub(super) results: Vec<MovieResult>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    total_pages: u32,
}

/// Entrée brute de `results` : AlloCiné envoie parfois `"movie": null` (séance IMAX sans
/// film rattaché, vue sur C0189 le 2026-10-08). Sans film, pas de `movie_id` : on l'écarte.
#[derive(Deserialize)]
struct RawMovieResult {
    movie: Option<Movie>,
    #[serde(default, deserialize_with = "null_as_default")]
    showtimes: BTreeMap<String, Vec<Showtime>>,
}

fn results_with_movie<'de, D>(deserializer: D) -> Result<Vec<MovieResult>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<RawMovieResult> = null_as_default(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|entry| {
            let Some(movie) = entry.movie else {
                tracing::debug!(
                    seances = entry.showtimes.values().map(Vec::len).sum::<usize>(),
                    "Entrée AlloCiné sans film ignorée"
                );
                return None;
            };
            Some(MovieResult {
                movie,
                showtimes: entry.showtimes,
            })
        })
        .collect())
}

pub(super) struct MovieResult {
    pub(super) movie: Movie,
    pub(super) showtimes: BTreeMap<String, Vec<Showtime>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Movie {
    pub(super) internal_id: i64,
    pub(super) title: String,
    pub(super) original_title: Option<String>,
    pub(super) poster: Option<Poster>,
    pub(super) synopsis: Option<String>,
    pub(super) runtime: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) languages: Vec<Option<String>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) genres: Vec<Genre>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) countries: Vec<Country>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) credits: Vec<Credit>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) cast: Cast,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) releases: Vec<Release>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) data: MovieData,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) stats: MovieStats,
}

#[derive(Deserialize)]
pub(super) struct Poster {
    pub(super) url: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct Genre {
    pub(super) translate: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Country {
    pub(super) localized_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Person {
    pub(super) first_name: Option<String>,
    pub(super) last_name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Credit {
    pub(super) person: Option<Person>,
    pub(super) position: Option<Position>,
}

#[derive(Default, Deserialize)]
pub(super) struct Position {
    pub(super) name: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct Cast {
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) edges: Vec<CastEdge>,
}

#[derive(Default, Deserialize)]
pub(super) struct CastEdge {
    pub(super) node: Option<CastNode>,
}

#[derive(Default, Deserialize)]
pub(super) struct CastNode {
    pub(super) actor: Option<Person>,
    pub(super) role: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct Release {
    #[serde(rename = "releaseDate")]
    pub(super) release_date: Option<ReleaseDate>,
    pub(super) certificate: Option<Certificate>,
}

#[derive(Default, Deserialize)]
pub(super) struct ReleaseDate {
    pub(super) date: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct Certificate {
    pub(super) label: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MovieData {
    pub(super) production_year: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MovieStats {
    pub(super) user_rating: Option<UserRating>,
}

#[derive(Default, Deserialize)]
pub(super) struct UserRating {
    pub(super) score: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Showtime {
    pub(super) internal_id: i64,
    pub(super) starts_at: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) diffusion_version: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) tags: Vec<String>,
    #[serde(default)]
    pub(super) projection: Option<Vec<String>>,
    #[serde(default)]
    pub(super) sound: Option<Vec<String>>,
    #[serde(default)]
    pub(super) picture: Option<Vec<String>>,
    #[serde(default)]
    pub(super) experience: Option<Vec<String>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) data: ShowtimeData,
}

#[derive(Default, Deserialize)]
pub(super) struct ShowtimeData {
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) ticketing: Vec<Ticketing>,
}

#[derive(Deserialize)]
pub(super) struct Ticketing {
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) urls: Vec<String>,
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) provider: String,
}

fn showtimes_url(cinema_id: &str, date: &str, page: u32) -> String {
    let page_path = if page == 1 {
        String::new()
    } else {
        format!("p-{page}/")
    };
    format!("{BASE_URL}/_/showtimes/theater-{cinema_id}/d-{date}/{page_path}")
}

fn allocine_request(client: &Client, url: &str) -> reqwest::RequestBuilder {
    client
        .get(url)
        .header(ACCEPT, "application/json")
        .header(reqwest::header::REFERER, format!("{BASE_URL}/"))
}

async fn fetch_page(
    client: &Client,
    cinema_id: &str,
    date: &str,
    page: u32,
    rate_limiter: &SharedRateLimiter,
    circuit_breaker: &CircuitBreaker,
) -> anyhow::Result<Response> {
    let url = showtimes_url(cinema_id, date, page);
    let bytes = fetch_with_retries_and_hooks(
        &url,
        &RETRY_DELAYS,
        || async {
            rate_limiter.wait().await;
            circuit_breaker.ensure_active()
        },
        |result| circuit_breaker.observe(result),
        || allocine_request(client, &url),
    )
    .await
    .with_context(|| format!("Téléchargement des séances {cinema_id} du {date}, page {page}"))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Désérialisation des séances {cinema_id} du {date}, page {page}"))
}

pub(super) async fn fetch_showtimes(
    client: &Client,
    cinema_id: &str,
    date: &str,
    rate_limiter: &SharedRateLimiter,
    circuit_breaker: &CircuitBreaker,
) -> anyhow::Result<Vec<MovieResult>> {
    let first_page = fetch_page(client, cinema_id, date, 1, rate_limiter, circuit_breaker).await?;
    if first_page.error {
        // `next.showtime.on` : rien ce jour-là, prochaine séance le `nextDate`.
        // `no.showtime.error` : aucune séance programmée du tout (`nextDate: null`).
        if matches!(
            first_page.message.as_deref(),
            Some("next.showtime.on" | "no.showtime.error")
        ) {
            tracing::info!(
                cinema_id,
                date,
                next_date = ?first_page.next_date,
                "Aucune séance pour cette date"
            );
            return Ok(Vec::new());
        }
        bail!(
            "Erreur AlloCiné pour le cinéma {cinema_id} le {date} : {}",
            first_page.message.as_deref().unwrap_or("message absent")
        );
    }

    let mut results = first_page.results;
    for page in 2..=first_page.pagination.total_pages.max(1) {
        let response =
            fetch_page(client, cinema_id, date, page, rate_limiter, circuit_breaker).await?;
        if response.error {
            bail!(
                "Erreur AlloCiné pour le cinéma {cinema_id} le {date}, page {page} : {}",
                response.message.as_deref().unwrap_or("message absent")
            );
        }
        results.extend(response.results);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::showtimes::mapping::booking_url;

    const C0159_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p1.json");
    const P0095_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-P0095-2026-10-06-p1.json");
    const FIXTURES: [&str; 5] = [
        C0159_PAGE_1,
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p2.json"),
        include_str!("../../tests/fixtures/showtimes-C0015-2026-10-06-p1.json"),
        include_str!("../../tests/fixtures/showtimes-P1434-2026-10-06-p1.json"),
        P0095_PAGE_1,
    ];

    #[test]
    fn showtimes_url_ends_with_a_slash_for_page_one() {
        assert_eq!(
            showtimes_url("C0159", "2026-10-07", 1),
            "https://www.allocine.fr/_/showtimes/theater-C0159/d-2026-10-07/"
        );
    }

    #[test]
    fn showtimes_url_adds_page_number_after_page_one() {
        assert_eq!(
            showtimes_url("C0159", "2026-10-07", 2),
            "https://www.allocine.fr/_/showtimes/theater-C0159/d-2026-10-07/p-2/"
        );
    }

    #[test]
    fn all_showtime_fixtures_deserialize() {
        for fixture in FIXTURES {
            serde_json::from_str::<Response>(fixture).unwrap();
        }
    }

    #[test]
    fn c0159_page_one_has_expected_movie_and_showtime_content() {
        let response: Response = serde_json::from_str(C0159_PAGE_1).unwrap();
        assert_eq!(response.results.len(), 15);
        assert_eq!(response.pagination.total_pages, 2);

        let entry = response
            .results
            .iter()
            .find(|entry| entry.movie.internal_id == 1000023992)
            .expect("film 1000023992 introuvable");
        assert_eq!(entry.movie.title, "Ni vue, ni connue");
        assert!(
            entry.showtimes.values().flatten().any(|showtime| {
                !showtime.starts_at.is_empty() && booking_url(showtime).is_some()
            })
        );
    }

    #[test]
    fn p0095_fixture_is_an_empty_error_response_with_next_date() {
        let response: Response = serde_json::from_str(P0095_PAGE_1).unwrap();
        assert!(response.error);
        assert!(response.results.is_empty());
        // nextDate is irrelevant to scraping logic; serde ignores this extra property.
        assert_eq!(response.message.as_deref(), Some("next.showtime.on"));
    }

    #[test]
    fn null_lists_deserialize_as_empty() {
        // Vu sur C0020 le 2026-10-08 : `"languages": null`.
        let json = r#"{
            "error": false,
            "pagination": { "totalPages": 1 },
            "results": [{
                "movie": {
                    "internalId": 1, "title": "Splendor", "originalTitle": null, "poster": null,
                    "synopsis": null, "runtime": null, "languages": null, "genres": null,
                    "countries": null, "credits": null, "cast": null, "releases": null,
                    "data": null, "stats": null
                },
                "showtimes": { "original": [{
                    "internalId": 2, "startsAt": "2026-10-08T20:00:00",
                    "diffusionVersion": "ORIGINAL", "tags": null,
                    "data": { "ticketing": [{ "urls": null, "type": "DESKTOP", "provider": "default" }] }
                }] }
            }]
        }"#;
        let response = serde_json::from_str::<Response>(json).unwrap();
        let entry = &response.results[0];
        assert!(entry.movie.languages.is_empty());
        assert!(entry.movie.cast.edges.is_empty());
        assert!(entry.showtimes["original"][0].tags.is_empty());
    }

    #[test]
    fn no_showtime_error_deserializes_as_empty_response() {
        // Vu sur C0030 le 2026-10-08 : cinéma sans aucune séance programmée.
        let json = r#"{"error":true,"message":"no.showtime.error","nextDate":null,"results":[]}"#;
        let response = serde_json::from_str::<Response>(json).unwrap();
        assert!(response.error);
        assert_eq!(response.message.as_deref(), Some("no.showtime.error"));
        assert!(response.results.is_empty());
    }

    #[test]
    fn entries_without_movie_are_skipped() {
        // Vu sur C0189 le 2026-10-08 : `"movie": null` avec une séance IMAX.
        let json = r#"{"error":false,"results":[
            {"movie":null,"showtimes":{"multiple":[{"internalId":1,"startsAt":"2026-10-09T10:00:00"}]}},
            {"movie":{"internalId":2,"title":"Film"},"showtimes":{}}
        ]}"#;
        let response = serde_json::from_str::<Response>(json).unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].movie.internal_id, 2);
    }
}
