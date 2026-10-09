use axum::{
    Json,
    extract::{Path, Query, State},
};
use sqlx::types::Json as SqlJson;

use super::AppState;
use super::error::{ApiResult, AppError};
use super::geo::{Position, haversine_km, round_km};
use super::params::{
    RawQuery, after_bound, parse_after, parse_bool, parse_cards, parse_date, parse_position,
    parse_version,
};
use super::types::{
    Cinema, CinemaShowtimesResponse, CinemaSummary, MovieShowtimes, MovieSummary, Showtime,
};
use crate::time::{DATE_FORMAT, paris_today};

/// Une ligne de `visible_cinemas`, telle que la base la stocke.
pub(super) struct CinemaRow {
    pub id: String,
    pub name: String,
    pub city: Option<String>,
    pub lat: f64,
    pub lng: f64,
    pub art_et_essai: bool,
    /// Ids des cartes, triés (ordre de la clé primaire `(cinema_id, card_id)`).
    pub cards: SqlJson<Vec<String>>,
}

impl CinemaRow {
    pub fn position(&self) -> Position {
        Position {
            lat: self.lat,
            lng: self.lng,
        }
    }

    pub fn into_summary(self, distance_km: Option<f64>) -> CinemaSummary {
        CinemaSummary {
            id: self.id,
            name: self.name,
            city: self.city,
            lat: self.lat,
            lng: self.lng,
            art_et_essai: self.art_et_essai,
            cards: self.cards.0,
            distance_km,
        }
    }
}

struct CinemaDetailRow {
    id: String,
    name: String,
    city: Option<String>,
    lat: f64,
    lng: f64,
    art_et_essai: bool,
    cards: SqlJson<Vec<String>>,
    address: Option<String>,
    postal_code: Option<String>,
    department: Option<String>,
    screens: Option<i64>,
    seats: Option<i64>,
}

/// Une séance jointe à son film, pour le programme d'un cinéma.
pub(super) struct MovieShowtimeRow {
    pub movie_id: i64,
    pub title: String,
    pub poster_url: Option<String>,
    pub genres: SqlJson<Vec<String>>,
    pub runtime_min: Option<i64>,
    pub release_date: Option<String>,
    pub showtime_id: String,
    pub starts_at: String,
    pub version: String,
    pub formats: SqlJson<Vec<String>>,
    pub booking_url: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<Vec<CinemaSummary>>> {
    let art_et_essai = parse_bool(raw.art_et_essai.as_deref(), "art_et_essai")?;
    let position = parse_position(raw.lat.as_deref(), raw.lng.as_deref())?;
    let cards = parse_cards(raw.cards.as_deref(), &state.pool).await?;

    let rows = sqlx::query_as!(
        CinemaRow,
        r#"SELECT c.id AS "id!: String", c.name AS "name!: String", c.city AS "city?: String",
                  c.lat AS "lat!: f64", c.lng AS "lng!: f64",
                  c.art_et_essai AS "art_et_essai!: bool",
                  (SELECT json_group_array(card_id) FROM cinema_cards WHERE cinema_id = c.id)
                    AS "cards!: SqlJson<Vec<String>>"
           FROM visible_cinemas c
           WHERE (?1 IS NULL OR c.art_et_essai = ?1)
             AND (?2 IS NULL OR EXISTS (
                   SELECT 1 FROM cinema_cards cc
                   WHERE cc.cinema_id = c.id
                     AND cc.card_id IN (SELECT value FROM json_each(?2))))
           ORDER BY c.name_search"#,
        art_et_essai,
        cards,
    )
    .fetch_all(&state.pool)
    .await?;

    // Une seule allocation, à la bonne taille (~3 100 éléments).
    let mut cinemas: Vec<CinemaSummary> = rows
        .into_iter()
        .map(|row| {
            let distance = position.map(|p| round_km(haversine_km(p, row.position())));
            row.into_summary(distance)
        })
        .collect();

    if position.is_some() {
        // Tri stable : à distance arrondie égale, l'ordre alphabétique est conservé.
        cinemas.sort_by(|a, b| {
            a.distance_km
                .unwrap_or_default()
                .total_cmp(&b.distance_km.unwrap_or_default())
        });
    }

    Ok(Json(cinemas))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Cinema>> {
    Ok(Json(fetch_cinema(&state, &id).await?))
}

/// Un cinéma visible, avec le nom de son département ; 404 sinon.
async fn fetch_cinema(state: &AppState, id: &str) -> ApiResult<Cinema> {
    let row = sqlx::query_as!(
        CinemaDetailRow,
        r#"SELECT c.id AS "id!: String", c.name AS "name!: String", c.city AS "city?: String",
                  c.lat AS "lat!: f64", c.lng AS "lng!: f64",
                  c.art_et_essai AS "art_et_essai!: bool",
                  (SELECT json_group_array(card_id) FROM cinema_cards WHERE cinema_id = c.id)
                    AS "cards!: SqlJson<Vec<String>>",
                  c.address AS "address?: String", c.postal_code AS "postal_code?: String",
                  c.department AS "department?: String", c.screens AS "screens?: i64",
                  c.seats AS "seats?: i64"
           FROM visible_cinemas c
           WHERE c.id = ?1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound("Cinéma introuvable"))?;

    let department = row
        .department
        .and_then(|code| state.departments.get(&code).cloned());
    let allocine_url = format!(
        "https://www.allocine.fr/seance/salle_gen_csalle={}.html",
        row.id
    );

    Ok(Cinema {
        summary: CinemaSummary {
            id: row.id,
            name: row.name,
            city: row.city,
            lat: row.lat,
            lng: row.lng,
            art_et_essai: row.art_et_essai,
            cards: row.cards.0,
            distance_km: None,
        },
        address: row.address,
        postal_code: row.postal_code,
        department,
        screens: row.screens,
        seats: row.seats,
        allocine_url,
    })
}

pub async fn showtimes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<CinemaShowtimesResponse>> {
    // 404 avant tout : un cinéma inconnu ou masqué n'a pas de programme.
    let cinema = fetch_cinema(&state, &id).await?;

    let today = paris_today();
    let date = parse_date(raw.date.as_deref(), today)?;
    let version = parse_version(raw.version.as_deref())?.map(|v| v.as_sql());
    let after = after_bound(date, parse_after(raw.after.as_deref())?);
    let date = date.format(DATE_FORMAT).to_string();
    let today = today.format(DATE_FORMAT).to_string();

    let rows = sqlx::query_as!(
        MovieShowtimeRow,
        r#"SELECT m.id AS "movie_id!: i64", m.title AS "title!: String",
                  m.poster_url AS "poster_url?: String",
                  m.genres AS "genres!: SqlJson<Vec<String>>",
                  m.runtime_min AS "runtime_min?: i64", m.release_date AS "release_date?: String",
                  s.id AS "showtime_id!: String", s.starts_at AS "starts_at!: String",
                  s.version AS "version!: String",
                  s.formats AS "formats!: SqlJson<Vec<String>>",
                  s.booking_url AS "booking_url?: String"
           FROM showtimes s
           JOIN movies m ON m.id = s.movie_id
           WHERE s.cinema_id = ?1
             AND s.date = ?2
             AND (?3 IS NULL OR s.version = ?3 OR (?3 = 'VO' AND s.version = 'VOST'))
             AND (?4 IS NULL OR s.starts_at >= ?4)
           ORDER BY m.title_search, m.id, s.starts_at"#,
        id,
        date,
        version,
        after,
    )
    .fetch_all(&state.pool)
    .await?;

    // Filtres version/after ignorés : sert à griser les jours vides du sélecteur.
    let dates = sqlx::query_scalar!(
        r#"SELECT DISTINCT date AS "date!: String"
           FROM showtimes
           WHERE cinema_id = ?1 AND date >= ?2
           ORDER BY date"#,
        id,
        today,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(CinemaShowtimesResponse {
        cinema,
        date,
        dates,
        movies: group_by_movie(rows),
    }))
}

/// Regroupe des lignes triées par film en une entrée par film, sans changer l'ordre.
fn group_by_movie(rows: Vec<MovieShowtimeRow>) -> Vec<MovieShowtimes> {
    let mut movies: Vec<MovieShowtimes> = Vec::new();
    for row in rows {
        let showtime = Showtime {
            id: row.showtime_id,
            starts_at: row.starts_at,
            version: row.version,
            formats: row.formats.0,
            booking_url: row.booking_url,
        };
        match movies.last_mut() {
            Some(last) if last.movie.id == row.movie_id => last.showtimes.push(showtime),
            _ => movies.push(MovieShowtimes {
                movie: MovieSummary {
                    id: row.movie_id,
                    title: row.title,
                    poster_url: row.poster_url,
                    genres: row.genres.0,
                    runtime_min: row.runtime_min,
                    release_date: row.release_date,
                },
                showtimes: vec![showtime],
            }),
        }
    }
    movies
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(movie_id: i64, showtime_id: &str) -> MovieShowtimeRow {
        MovieShowtimeRow {
            movie_id,
            title: format!("Film {movie_id}"),
            poster_url: None,
            genres: SqlJson(vec!["Drame".into()]),
            runtime_min: None,
            release_date: None,
            showtime_id: showtime_id.into(),
            starts_at: "2026-10-08T20:00:00".into(),
            version: "VF".into(),
            formats: SqlJson(vec![]),
            booking_url: None,
        }
    }

    #[test]
    fn group_by_movie_keeps_order() {
        let groups = group_by_movie(vec![row(2, "a"), row(2, "b"), row(1, "c")]);
        let summary: Vec<(i64, Vec<&str>)> = groups
            .iter()
            .map(|g| {
                (
                    g.movie.id,
                    g.showtimes.iter().map(|s| s.id.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(summary, [(2, vec!["a", "b"]), (1, vec!["c"])]);
        assert_eq!(groups[0].movie.genres, ["Drame"]);
    }

    #[test]
    fn group_by_movie_of_nothing_is_empty() {
        assert!(group_by_movie(Vec::new()).is_empty());
    }
}
