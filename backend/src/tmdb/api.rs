//! Appels à l'API TMDB v3 et désérialisation des réponses utiles.

use std::time::Duration;

use anyhow::Context;
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use tokio::time::{Instant, sleep_until};

use crate::client::{RETRY_DELAYS, fetch_with_retries};

const BASE_URL: &str = "https://api.themoviedb.org/3";
const LANGUAGE: &str = "fr-FR";
/// TMDB tolère ~50 req/s ; on reste loin en dessous, la latence fait le reste.
const REQUEST_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SearchResult {
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub original_title: String,
    /// "2024-05-01", "" ou absent quand TMDB ne la connaît pas.
    pub release_date: Option<String>,
}

impl SearchResult {
    pub fn release_year(&self) -> Option<i64> {
        self.release_date.as_deref()?.get(..4)?.parse().ok()
    }
}

#[derive(Debug, Deserialize)]
pub struct MovieDetails {
    pub id: i64,
    pub backdrop_path: Option<String>,
    #[serde(default)]
    pub vote_average: f64,
    #[serde(default)]
    pub vote_count: i64,
    #[serde(default)]
    pub videos: Videos,
}

#[derive(Debug, Default, Deserialize)]
pub struct Videos {
    #[serde(default)]
    pub results: Vec<Video>,
}

#[derive(Debug, Deserialize)]
pub struct Video {
    pub key: String,
    pub site: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub official: bool,
    pub iso_639_1: Option<String>,
}

/// Client TMDB : clé, limite de débit. La clé peut être la « clé d'API » (v3, envoyée
/// en `api_key`) ou le « jeton d'accès en lecture » (JWT, envoyé en `Bearer`).
pub struct Tmdb {
    client: Client,
    key: String,
    next_request_at: Instant,
}

impl Tmdb {
    pub fn new(client: Client, key: String) -> Self {
        Tmdb {
            client,
            key,
            next_request_at: Instant::now(),
        }
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        if self.key.starts_with("eyJ") {
            request.bearer_auth(&self.key)
        } else {
            request.query(&[("api_key", self.key.as_str())])
        }
    }

    async fn get(&mut self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<bytes::Bytes> {
        sleep_until(self.next_request_at).await;
        self.next_request_at = Instant::now() + REQUEST_INTERVAL;
        let url = format!("{BASE_URL}{path}");
        // La clé n'apparaît pas dans `url` : les messages d'erreur ne la divulguent pas.
        fetch_with_retries(&url, &RETRY_DELAYS, || {
            self.authorized(self.client.get(&url).query(query))
        })
        .await
    }

    pub async fn search(&mut self, query: &str) -> anyhow::Result<Vec<SearchResult>> {
        let body = self
            .get(
                "/search/movie",
                &[
                    ("query", query),
                    ("language", LANGUAGE),
                    ("include_adult", "false"),
                ],
            )
            .await?;
        let response: SearchResponse = serde_json::from_slice(&body)
            .with_context(|| format!("Réponse de recherche TMDB illisible pour {query:?}"))?;
        Ok(response.results)
    }

    pub async fn details(&mut self, tmdb_id: i64) -> anyhow::Result<MovieDetails> {
        let body = self
            .get(
                &format!("/movie/{tmdb_id}"),
                &[
                    ("language", LANGUAGE),
                    ("append_to_response", "videos"),
                    // Sans ce paramètre, `videos` ne contient que la langue demandée.
                    ("include_video_language", "fr,en,null"),
                ],
            )
            .await?;
        serde_json::from_slice(&body).with_context(|| format!("Fiche TMDB {tmdb_id} illisible"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_response_tolerates_missing_and_empty_dates() {
        let response: SearchResponse = serde_json::from_str(
            r#"{"page":1,"results":[
                {"id":1,"title":"Dune : Deuxième partie","original_title":"Dune: Part Two","release_date":"2024-02-27","popularity":12.3},
                {"id":2,"title":"Sans date","original_title":"Sans date","release_date":""},
                {"id":3,"title":"Absente","original_title":"Absente"}
            ],"total_pages":1,"total_results":3}"#,
        )
        .unwrap();
        let years: Vec<Option<i64>> = response.results.iter().map(|r| r.release_year()).collect();
        assert_eq!(years, [Some(2024), None, None]);
    }

    #[test]
    fn details_reads_backdrop_votes_and_videos() {
        let details: MovieDetails = serde_json::from_str(
            r#"{"id":693134,"backdrop_path":"/xOMo8BRK7PfcJv9JCnx7s5hj0PX.jpg","runtime":167,
                "vote_average":8.1,"vote_count":6000,
                "videos":{"results":[
                    {"iso_639_1":"fr","iso_3166_1":"FR","name":"Bande-annonce VF","key":"abc","site":"YouTube","type":"Trailer","official":true,"published_at":"2024-01-01T00:00:00.000Z","id":"x"}
                ]}}"#,
        )
        .unwrap();
        assert_eq!(details.id, 693134);
        assert_eq!(details.videos.results[0].kind, "Trailer");
        assert_eq!(details.vote_count, 6000);
    }

    #[test]
    fn details_without_videos_or_backdrop() {
        let details: MovieDetails = serde_json::from_str(
            r#"{"id":1,"backdrop_path":null,"vote_average":0.0,"vote_count":0}"#,
        )
        .unwrap();
        assert!(details.videos.results.is_empty());
        assert!(details.backdrop_path.is_none());
    }
}
