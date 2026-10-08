use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use sqlx::{SqlitePool, types::Json as SqlJson};

use super::AppState;
use super::cinemas::CinemaRow;
use super::error::{ApiResult, AppError};
use super::geo::{Position, bounding_box, haversine_km, round_km};
use super::params::{
    RawQuery, after_bound, parse_after, parse_date, parse_limit, parse_movie_id, parse_position,
    parse_radius, parse_version,
};
use super::types::{
    CinemaSummary, CinemaWithShowtimes, Movie, MovieShowtimesResponse, MovieSummary, NowShowing,
    NowShowingResponse, Person, Showtime,
};
use crate::time::{DATE_FORMAT, paris_today};

/// Colonnes de `MovieSummary`, telles que la base les stocke.
pub(super) struct MovieSummaryRow {
    pub id: i64,
    pub title: String,
    pub poster_url: Option<String>,
    pub genres: SqlJson<Vec<String>>,
    pub runtime_min: Option<i64>,
    pub release_date: Option<String>,
}

impl From<MovieSummaryRow> for MovieSummary {
    fn from(row: MovieSummaryRow) -> Self {
        MovieSummary {
            id: row.id,
            title: row.title,
            poster_url: row.poster_url,
            genres: row.genres.0,
            runtime_min: row.runtime_min,
            release_date: row.release_date,
        }
    }
}

struct MovieRow {
    id: i64,
    title: String,
    poster_url: Option<String>,
    genres: SqlJson<Vec<String>>,
    runtime_min: Option<i64>,
    release_date: Option<String>,
    original_title: Option<String>,
    synopsis: Option<String>,
    directors: SqlJson<Vec<String>>,
    cast_members: SqlJson<Vec<Person>>,
    countries: SqlJson<Vec<String>>,
    production_year: Option<i64>,
    certificate: Option<String>,
    backdrop_url: Option<String>,
    trailer_url: Option<String>,
    rating: Option<f64>,
    user_rating: Option<f64>,
}

struct CinemaShowtimeRow {
    cinema_id: String,
    showtime_id: String,
    starts_at: String,
    version: String,
    formats: SqlJson<Vec<String>>,
    booking_url: Option<String>,
}

struct NowShowingRow {
    id: i64,
    title: String,
    poster_url: Option<String>,
    genres: SqlJson<Vec<String>>,
    runtime_min: Option<i64>,
    release_date: Option<String>,
    cinema_count: i64,
    showtime_count: i64,
    next_showtime: String,
}

pub(super) struct NearbyCinema {
    /// `distance_km` rempli (arrondi).
    pub summary: CinemaSummary,
    /// Distance exacte, pour filtrer et trier.
    pub distance: f64,
}

/// Cinémas visibles à `radius_km` au plus de `center`, du plus proche au plus loin.
pub(super) async fn cinemas_within(
    pool: &SqlitePool,
    center: Position,
    radius_km: f64,
) -> ApiResult<Vec<NearbyCinema>> {
    let bbox = bounding_box(center, radius_km);
    let rows = sqlx::query_as!(
        CinemaRow,
        r#"SELECT id AS "id!: String", name AS "name!: String", city AS "city?: String",
                  lat AS "lat!: f64", lng AS "lng!: f64", art_et_essai AS "art_et_essai!: bool"
           FROM visible_cinemas
           WHERE lat BETWEEN ?1 AND ?2 AND lng BETWEEN ?3 AND ?4"#,
        bbox.min_lat,
        bbox.max_lat,
        bbox.min_lng,
        bbox.max_lng,
    )
    .fetch_all(pool)
    .await?;

    let mut nearby: Vec<NearbyCinema> = rows
        .into_iter()
        .filter_map(|row| {
            let distance = haversine_km(center, row.position());
            (distance <= radius_km).then(|| NearbyCinema {
                summary: row.into_summary(Some(round_km(distance))),
                distance,
            })
        })
        .collect();
    nearby.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    Ok(nearby)
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Movie>> {
    let id = parse_movie_id(&id)?;
    // Un film sans séance reste consultable : un lien partagé hier doit marcher.
    let row = sqlx::query_as!(
        MovieRow,
        r#"SELECT id AS "id!: i64", title AS "title!: String", poster_url AS "poster_url?: String",
                  genres AS "genres!: SqlJson<Vec<String>>",
                  runtime_min AS "runtime_min?: i64", release_date AS "release_date?: String",
                  original_title AS "original_title?: String", synopsis AS "synopsis?: String",
                  directors AS "directors!: SqlJson<Vec<String>>",
                  cast_members AS "cast_members!: SqlJson<Vec<Person>>",
                  countries AS "countries!: SqlJson<Vec<String>>",
                  production_year AS "production_year?: i64", certificate AS "certificate?: String",
                  backdrop_url AS "backdrop_url?: String", trailer_url AS "trailer_url?: String",
                  rating AS "rating?: f64", user_rating AS "user_rating?: f64"
           FROM movies
           WHERE id = ?1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound("Film introuvable"))?;

    Ok(Json(Movie {
        summary: MovieSummary {
            id: row.id,
            title: row.title,
            poster_url: row.poster_url,
            genres: row.genres.0,
            runtime_min: row.runtime_min,
            release_date: row.release_date,
        },
        original_title: row.original_title,
        synopsis: row.synopsis,
        directors: row.directors.0,
        cast: row.cast_members.0,
        countries: row.countries.0,
        production_year: row.production_year,
        certificate: row.certificate,
        backdrop_url: row.backdrop_url,
        trailer_url: row.trailer_url,
        rating: row.rating,
        user_rating: row.user_rating,
    }))
}

pub async fn showtimes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<MovieShowtimesResponse>> {
    let id = parse_movie_id(&id)?;
    let center = parse_position(raw.lat.as_deref(), raw.lng.as_deref())?
        .ok_or_else(|| AppError::BadRequest("lat et lng sont obligatoires".into()))?;
    let radius_km = parse_radius(raw.radius_km.as_deref())?;
    let today = paris_today();
    let date = parse_date(raw.date.as_deref(), today)?;
    let version = parse_version(raw.version.as_deref())?.map(|v| v.as_sql());
    let after = after_bound(date, parse_after(raw.after.as_deref())?);
    let date = date.format(DATE_FORMAT).to_string();
    let today = today.format(DATE_FORMAT).to_string();

    let movie: MovieSummary = sqlx::query_as!(
        MovieSummaryRow,
        r#"SELECT id AS "id!: i64", title AS "title!: String", poster_url AS "poster_url?: String",
                  genres AS "genres!: SqlJson<Vec<String>>",
                  runtime_min AS "runtime_min?: i64", release_date AS "release_date?: String"
           FROM movies
           WHERE id = ?1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound("Film introuvable"))?
    .into();

    let nearby = cinemas_within(&state.pool, center, radius_km).await?;
    if nearby.is_empty() {
        return Ok(Json(MovieShowtimesResponse {
            movie,
            date,
            dates: Vec::new(),
            cinemas: Vec::new(),
        }));
    }
    // sqlx ne sait pas lier un Vec à `IN (?)` avec SQLite : la liste passe en
    // JSON et `json_each` la redéroule côté SQL.
    let ids: Vec<&str> = nearby.iter().map(|c| c.summary.id.as_str()).collect();
    let ids = serde_json::to_string(&ids)?;

    let rows = sqlx::query_as!(
        CinemaShowtimeRow,
        r#"SELECT s.cinema_id AS "cinema_id!: String", s.id AS "showtime_id!: String",
                  s.starts_at AS "starts_at!: String", s.version AS "version!: String",
                  s.formats AS "formats!: SqlJson<Vec<String>>",
                  s.booking_url AS "booking_url?: String"
           FROM showtimes s
           WHERE s.movie_id = ?1
             AND s.date = ?2
             AND s.cinema_id IN (SELECT value FROM json_each(?3))
             AND (?4 IS NULL OR s.version = ?4 OR (?4 = 'VO' AND s.version = 'VOST'))
             AND (?5 IS NULL OR s.starts_at >= ?5)
           ORDER BY s.cinema_id, s.starts_at"#,
        id,
        date,
        ids,
        version,
        after,
    )
    .fetch_all(&state.pool)
    .await?;

    let dates = sqlx::query_scalar!(
        r#"SELECT DISTINCT date AS "date!: String"
           FROM showtimes
           WHERE movie_id = ?1
             AND date >= ?2
             AND cinema_id IN (SELECT value FROM json_each(?3))
           ORDER BY date"#,
        id,
        today,
        ids,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut by_cinema: HashMap<String, Vec<Showtime>> = HashMap::new();
    for row in rows {
        by_cinema.entry(row.cinema_id).or_default().push(Showtime {
            id: row.showtime_id,
            starts_at: row.starts_at,
            version: row.version,
            formats: row.formats.0,
            booking_url: row.booking_url,
        });
    }

    // `nearby` est déjà trié par distance : l'ordre du contrat est gratuit.
    let cinemas = nearby
        .into_iter()
        .filter_map(|cinema| {
            by_cinema
                .remove(&cinema.summary.id)
                .map(|showtimes| CinemaWithShowtimes {
                    cinema: cinema.summary,
                    showtimes,
                })
        })
        .collect();

    Ok(Json(MovieShowtimesResponse {
        movie,
        date,
        dates,
        cinemas,
    }))
}

pub async fn list(
    State(state): State<AppState>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<NowShowingResponse>> {
    let date = parse_date(raw.date.as_deref(), paris_today())?;
    let position = parse_position(raw.lat.as_deref(), raw.lng.as_deref())?;
    let radius_km = parse_radius(raw.radius_km.as_deref())?;
    let version = parse_version(raw.version.as_deref())?.map(|v| v.as_sql());
    let after = after_bound(date, parse_after(raw.after.as_deref())?);
    let limit = i64::from(parse_limit(raw.limit.as_deref())?);
    let date = date.format(DATE_FORMAT).to_string();

    let cinema_ids = match position {
        None => None,
        Some(center) => {
            let nearby = cinemas_within(&state.pool, center, radius_km).await?;
            if nearby.is_empty() {
                // Rien dans le rayon : inutile d'interroger les séances.
                return Ok(Json(NowShowingResponse {
                    date,
                    movies: Vec::new(),
                }));
            }
            let ids: Vec<&str> = nearby.iter().map(|c| c.summary.id.as_str()).collect();
            Some(serde_json::to_string(&ids)?)
        }
    };

    // Tri sur les agrégats répétés : un alias "x!: T" contient le type, on ne
    // peut pas le citer dans ORDER BY.
    let rows = sqlx::query_as!(
        NowShowingRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  m.poster_url AS "poster_url?: String",
                  m.genres AS "genres!: SqlJson<Vec<String>>",
                  m.runtime_min AS "runtime_min?: i64", m.release_date AS "release_date?: String",
                  count(DISTINCT s.cinema_id) AS "cinema_count!: i64",
                  count(*) AS "showtime_count!: i64",
                  min(s.starts_at) AS "next_showtime!: String"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           JOIN movies m ON m.id = s.movie_id
           WHERE s.date = ?1
             AND (?2 IS NULL OR s.cinema_id IN (SELECT value FROM json_each(?2)))
             AND (?3 IS NULL OR s.version = ?3 OR (?3 = 'VO' AND s.version = 'VOST'))
             AND (?4 IS NULL OR s.starts_at >= ?4)
           GROUP BY m.id
           ORDER BY count(DISTINCT s.cinema_id) DESC, count(*) DESC, m.title_search
           LIMIT ?5"#,
        date,
        cinema_ids,
        version,
        after,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;

    let movies = rows
        .into_iter()
        .map(|row| NowShowing {
            movie: MovieSummary {
                id: row.id,
                title: row.title,
                poster_url: row.poster_url,
                genres: row.genres.0,
                runtime_min: row.runtime_min,
                release_date: row.release_date,
            },
            cinema_count: row.cinema_count,
            showtime_count: row.showtime_count,
            next_showtime: row.next_showtime,
        })
        .collect();

    Ok(Json(NowShowingResponse { date, movies }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_cinemas(cinemas: &[(&str, f64)]) -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        for (id, km_north) in cinemas {
            sqlx::query(
                "INSERT INTO cinemas (id, name, name_search, lat, lng, updated_at)
                 VALUES (?1, ?1, ?1, ?2, 2.3522, datetime('now'))",
            )
            .bind(id)
            .bind(48.8566 + km_north / 111.32)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn cinemas_within_filters_by_radius_and_sorts_by_distance() {
        let pool = pool_with_cinemas(&[("FAR", 20.0), ("MID", 14.0), ("NEAR", 1.0)]).await;
        let center = Position {
            lat: 48.8566,
            lng: 2.3522,
        };
        let nearby = cinemas_within(&pool, center, 15.0).await.unwrap();
        let ids: Vec<&str> = nearby.iter().map(|c| c.summary.id.as_str()).collect();
        assert_eq!(ids, ["NEAR", "MID"]);
        assert_eq!(nearby[0].summary.distance_km, Some(1.0));
    }
}
