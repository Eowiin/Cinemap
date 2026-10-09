use std::collections::BTreeMap;

use crate::client::{RETRY_DELAYS, fetch_with_retries_and_hooks};
use anyhow::{Context, bail};
use chrono::NaiveDate;
use reqwest::{Client, header::ACCEPT};
use serde::Deserialize;

use super::{CircuitBreaker, SharedRateLimiter, observe_response};
use crate::time::DATE_FORMAT;

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

/// Liste de chaînes dont les éléments `null` sont écartés : `"projection": [null]` vu sur
/// W5076 et P0535 le 2026-10-08 (le run échouait sur tout le couple cinéma/date).
fn strings_without_nulls<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<Option<String>> = null_as_default(deserializer)?;
    Ok(values.into_iter().flatten().collect())
}

/// Comme `strings_without_nulls`, en gardant la distinction absent / `null` → `None`.
fn optional_strings_without_nulls<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Option::<Vec<Option<String>>>::deserialize(deserializer)?;
    Ok(values.map(|values| values.into_iter().flatten().collect()))
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

/// Une sortie du film : en salle (`Released` / `ReRelease` + `THEATER`), mais aussi VOD,
/// DVD, télévision… dans un ordre quelconque.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Release {
    pub(super) release_date: Option<ReleaseDate>,
    pub(super) certificate: Option<Certificate>,
    /// `Released`, `ReRelease`, `Vod`, `Dvd`…
    pub(super) name: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(super) release_tags: ReleaseTags,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReleaseTags {
    #[serde(default, deserialize_with = "strings_without_nulls")]
    pub(super) tag_types: Vec<String>,
}

impl Release {
    fn in_theaters(&self) -> bool {
        self.release_tags
            .tag_types
            .iter()
            .any(|tag| tag == "THEATER")
    }

    fn date(&self) -> Option<&str> {
        self.release_date.as_ref()?.date.as_deref()
    }
}

/// La sortie qui date le film : la sortie d'origine en salle, sinon la plus ancienne
/// sortie en salle (ressortie), sinon la plus ancienne tout court. Prendre la première
/// de la liste donnait parfois une ressortie (Godzilla Minus One, 2023, daté 2026).
pub(super) fn original_release(releases: &[Release]) -> Option<&Release> {
    let original = |r: &&Release| r.in_theaters() && r.name.as_deref() == Some("Released");
    earliest(releases.iter().filter(original))
        .or_else(|| earliest(releases.iter().filter(|r| r.in_theaters())))
        .or_else(|| earliest(releases.iter()))
}

/// La sortie datée la plus ancienne (les dates `YYYY-MM-DD` se comparent comme du texte).
fn earliest<'a>(releases: impl Iterator<Item = &'a Release>) -> Option<&'a Release> {
    releases
        .filter(|r| r.date().is_some())
        .min_by_key(|r| r.date())
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
    #[serde(default, deserialize_with = "strings_without_nulls")]
    pub(super) tags: Vec<String>,
    #[serde(default, deserialize_with = "optional_strings_without_nulls")]
    pub(super) projection: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_strings_without_nulls")]
    pub(super) sound: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_strings_without_nulls")]
    pub(super) picture: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_strings_without_nulls")]
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
    #[serde(default, deserialize_with = "strings_without_nulls")]
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
        |result| observe_response(result, rate_limiter, circuit_breaker),
        || allocine_request(client, &url),
    )
    .await
    .with_context(|| format!("Téléchargement des séances {cinema_id} du {date}, page {page}"))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Désérialisation des séances {cinema_id} du {date}, page {page}"))
}

/// Séances d'un cinéma pour un jour.
pub(super) enum Day {
    Showtimes(Vec<MovieResult>),
    /// Aucune séance ce jour-là. `empty_until` : AlloCiné garantit qu'il n'y en a pas non plus
    /// avant cette date (exclue) ; `None` si on ne sait rien des jours suivants.
    Empty {
        empty_until: Option<NaiveDate>,
    },
}

/// Jusqu'à quand (exclu) un cinéma sans séance ce jour-là n'en a aucune, d'après AlloCiné.
fn empty_until(message: Option<&str>, next_date: Option<&str>) -> Option<NaiveDate> {
    match (message, next_date) {
        // Aucune séance programmée du tout.
        (Some("no.showtime.error"), _) => Some(NaiveDate::MAX),
        // Rien avant la prochaine séance. Date illisible ou absente : on ne saute rien.
        (Some("next.showtime.on"), Some(next)) => NaiveDate::parse_from_str(next, DATE_FORMAT).ok(),
        _ => None,
    }
}

pub(super) async fn fetch_showtimes(
    client: &Client,
    cinema_id: &str,
    date: &str,
    rate_limiter: &SharedRateLimiter,
    circuit_breaker: &CircuitBreaker,
) -> anyhow::Result<Day> {
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
            return Ok(Day::Empty {
                empty_until: empty_until(
                    first_page.message.as_deref(),
                    first_page.next_date.as_deref(),
                ),
            });
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
            // La page 1 annonçait plus de pages, mais les séances ont changé entre-temps (vu
            // le soir, quand les dernières séances du jour passent) : on garde ce qu'on a.
            if matches!(
                response.message.as_deref(),
                Some("next.showtime.on" | "no.showtime.error")
            ) {
                tracing::info!(
                    cinema_id,
                    date,
                    page,
                    "Page suivante vide : pagination arrêtée"
                );
                break;
            }
            bail!(
                "Erreur AlloCiné pour le cinéma {cinema_id} le {date}, page {page} : {}",
                response.message.as_deref().unwrap_or("message absent")
            );
        }
        results.extend(response.results);
    }
    Ok(Day::Showtimes(results))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_release_is_the_first_theatrical_release() {
        // Comme Godzilla Minus One : VOD et ressortie en salle 2026 avant la sortie de 2023.
        let releases: Vec<Release> = serde_json::from_str(
            r#"[
              {"name": "Vod", "releaseDate": {"date": "2024-03-01"}, "releaseTags": {"tagTypes": ["ONLINE"]}},
              {"name": "ReRelease", "releaseDate": {"date": "2026-10-11"}, "releaseTags": {"tagTypes": ["THEATER"]},
               "certificate": {"label": "Tout public"}},
              {"name": "Released", "releaseDate": {"date": "2023-12-07"}, "releaseTags": {"tagTypes": ["THEATER"]},
               "certificate": {"label": "Avertissement"}}
            ]"#,
        )
        .unwrap();
        let release = original_release(&releases).unwrap();
        assert_eq!(release.date(), Some("2023-12-07"));
        assert_eq!(
            release.certificate.as_ref().unwrap().label.as_deref(),
            Some("Avertissement")
        );

        // Sans sortie d'origine : la plus ancienne en salle, puis la plus ancienne tout court.
        assert_eq!(
            original_release(&releases[..2]).unwrap().date(),
            Some("2026-10-11")
        );
        assert_eq!(
            original_release(&releases[..1]).unwrap().date(),
            Some("2024-03-01")
        );
        assert!(original_release(&[]).is_none());
    }
    use crate::showtimes::mapping::booking_url;

    const C0159_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p1.json");
    const P0095_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-P0095-2026-10-06-p1.json");
    const W5076_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-W5076-2026-10-09-p1.json");
    const FIXTURES: [&str; 6] = [
        C0159_PAGE_1,
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p2.json"),
        include_str!("../../tests/fixtures/showtimes-C0015-2026-10-06-p1.json"),
        include_str!("../../tests/fixtures/showtimes-P1434-2026-10-06-p1.json"),
        P0095_PAGE_1,
        W5076_PAGE_1,
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
        assert_eq!(response.message.as_deref(), Some("next.showtime.on"));
        assert_eq!(
            empty_until(response.message.as_deref(), response.next_date.as_deref()),
            NaiveDate::from_ymd_opt(2026, 10, 7)
        );
    }

    #[test]
    fn empty_until_skips_nothing_when_unsure() {
        assert_eq!(
            empty_until(Some("no.showtime.error"), None),
            Some(NaiveDate::MAX)
        );
        assert_eq!(empty_until(Some("next.showtime.on"), None), None);
        assert_eq!(empty_until(Some("next.showtime.on"), Some("demain")), None);
        assert_eq!(empty_until(Some("autre.message"), Some("2026-10-07")), None);
        assert_eq!(empty_until(None, None), None);
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
    fn null_list_elements_are_dropped() {
        // Vu sur W5076 le 2026-10-09 : `"projection": [null]`.
        let response: Response = serde_json::from_str(W5076_PAGE_1).unwrap();
        let showtimes: Vec<&Showtime> = response
            .results
            .iter()
            .flat_map(|entry| entry.showtimes.values().flatten())
            .collect();
        assert!(
            showtimes
                .iter()
                .any(|showtime| showtime.projection == Some(Vec::new()))
        );

        let json = r#"{"internalId": 1, "startsAt": "2026-10-09T20:00:00", "tags": [null, "Tag"],
            "projection": [null, "3D"], "sound": null,
            "data": {"ticketing": [{"urls": [null, "https://example.org"], "type": "DESKTOP", "provider": "default"}]}}"#;
        let showtime: Showtime = serde_json::from_str(json).unwrap();
        assert_eq!(showtime.tags, ["Tag"]);
        assert_eq!(showtime.projection, Some(vec!["3D".to_owned()]));
        assert_eq!(showtime.sound, None);
        assert_eq!(showtime.picture, None);
        assert_eq!(showtime.data.ticketing[0].urls, ["https://example.org"]);
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
