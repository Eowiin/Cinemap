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
    pub cards: Vec<String>,
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
    pub city_slug: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CitySummary {
    pub slug: String,
    pub name: String,
    pub label: String,
    pub department: Option<String>,
    pub cinema_count: i64,
}

#[derive(Debug, Serialize)]
pub struct City {
    #[serde(flatten)]
    pub summary: CitySummary,
    pub lat: f64,
    pub lng: f64,
    pub cinemas: Vec<CinemaSummary>,
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
    /// Au moins une séance aujourd'hui ou plus tard, dans n'importe quel cinéma.
    pub showing: bool,
    pub last_showtime_date: Option<String>,
    /// `null` tant que TMDB n'a pas été lu (ou film non trouvé sur TMDB).
    pub watch: Option<Watch>,
}

/// Où voir un film en France, par type d'offre (colonne JSON `watch_providers`,
/// écrite par la synchro TMDB). Chaque liste est triée par l'ordre JustWatch.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchProviders {
    /// Page TMDB qui détaille les offres.
    pub link: Option<String>,
    /// Gratuit, avec ou sans publicité (arte.tv, france.tv…).
    pub free: Vec<WatchProvider>,
    /// Inclus dans un abonnement.
    pub flatrate: Vec<WatchProvider>,
    pub rent: Vec<WatchProvider>,
    pub buy: Vec<WatchProvider>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct WatchProvider {
    pub name: String,
    pub logo_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Watch {
    #[serde(flatten)]
    pub providers: WatchProviders,
    /// Date et heure (UTC, `YYYY-MM-DD HH:MM:SS`) de la dernière lecture TMDB.
    pub checked_at: String,
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
pub struct SearchMovie {
    #[serde(flatten)]
    pub movie: MovieSummary,
    pub showing: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub movies: Vec<SearchMovie>,
    pub cinemas: Vec<CinemaSummary>,
    pub cities: Vec<CitySummary>,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub today: String,
    pub last_scrape_at: Option<String>,
    pub dates_available: Vec<String>,
    pub cinema_count: i64,
    pub movie_count: i64,
    pub cards: Vec<Card>,
}

#[derive(Debug, Serialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub updated_at: String,
}

/// Séance de `/api/showtimes` : le film et le cinéma sont donnés à part, une fois chacun.
#[derive(Debug, Serialize)]
pub struct NearbyShowtime {
    #[serde(flatten)]
    pub showtime: Showtime,
    pub cinema_id: String,
    pub movie_id: i64,
}

#[derive(Debug, Serialize)]
pub struct ShowtimesResponse {
    pub date: String,
    pub showtimes: Vec<NearbyShowtime>,
    pub cinemas: Vec<CinemaSummary>,
    pub movies: Vec<MovieSummary>,
}
