//! Types de réponse : les types TypeScript de `docs/API.md`, traduits en Rust.
//! Pas de `skip_serializing_if` : le contrat veut `null`, jamais un champ absent.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct CinemaSummary {
    pub id: String,
    pub name: String,
    pub city: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub art_et_essai: bool,
    pub distance_km: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Cinema {
    #[serde(flatten)]
    pub summary: CinemaSummary,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub department: Option<String>,
    pub screens: Option<i64>,
    pub seats: Option<i64>,
    pub allocine_url: String,
}

#[derive(Debug, Serialize)]
pub struct MovieSummary {
    pub id: i64,
    pub title: String,
    pub poster_url: Option<String>,
    pub genres: Vec<String>,
    pub runtime_min: Option<i64>,
    pub release_date: Option<String>,
}

/// Lue depuis la colonne JSON `cast_members`, renvoyée telle quelle.
#[derive(Debug, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Movie {
    #[serde(flatten)]
    pub summary: MovieSummary,
    pub original_title: Option<String>,
    pub synopsis: Option<String>,
    pub directors: Vec<String>,
    pub cast: Vec<Person>,
    pub countries: Vec<String>,
    pub production_year: Option<i64>,
    pub certificate: Option<String>,
    pub backdrop_url: Option<String>,
    pub trailer_url: Option<String>,
    pub rating: Option<f64>,
    pub user_rating: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Showtime {
    pub id: String,
    pub starts_at: String,
    pub version: String,
    pub formats: Vec<String>,
    pub booking_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MovieShowtimes {
    pub movie: MovieSummary,
    pub showtimes: Vec<Showtime>,
}

#[derive(Debug, Serialize)]
pub struct CinemaShowtimesResponse {
    pub cinema: Cinema,
    pub date: String,
    pub dates: Vec<String>,
    pub movies: Vec<MovieShowtimes>,
}

#[derive(Debug, Serialize)]
pub struct CinemaWithShowtimes {
    pub cinema: CinemaSummary,
    pub showtimes: Vec<Showtime>,
}

#[derive(Debug, Serialize)]
pub struct MovieShowtimesResponse {
    pub movie: MovieSummary,
    pub date: String,
    pub dates: Vec<String>,
    pub cinemas: Vec<CinemaWithShowtimes>,
}

#[derive(Debug, Serialize)]
pub struct NowShowing {
    pub movie: MovieSummary,
    pub cinema_count: i64,
    pub showtime_count: i64,
    pub next_showtime: String,
}

#[derive(Debug, Serialize)]
pub struct NowShowingResponse {
    pub date: String,
    pub movies: Vec<NowShowing>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub movies: Vec<MovieSummary>,
    pub cinemas: Vec<CinemaSummary>,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub today: String,
    pub last_scrape_at: Option<String>,
    pub dates_available: Vec<String>,
    pub cinema_count: i64,
    pub movie_count: i64,
}
