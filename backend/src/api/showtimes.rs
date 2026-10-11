//! `GET /api/showtimes` : les prochaines séances autour d'un point, tous films
//! confondus (« Et après ? », puis « Ce soir près de moi »).

use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Query, State},
};
use sqlx::types::Json as SqlJson;

use super::AppState;
use super::error::{ApiResult, AppError};
use super::movies::{MovieSummaryRow, cinemas_by_ids, cinemas_within};
use super::params::{
    RawQuery, after_bound, parse_after, parse_cards, parse_cinemas, parse_date, parse_limit,
    parse_position, parse_radius, parse_version,
};
use super::types::{MovieSummary, NearbyShowtime, Showtime, ShowtimesResponse};
use crate::time::{DATE_FORMAT, paris_today};

struct ShowtimeRow {
    id: String,
    cinema_id: String,
    movie_id: i64,
    starts_at: String,
    version: String,
    formats: SqlJson<Vec<String>>,
    booking_url: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<ShowtimesResponse>> {
    let center = parse_position(raw.lat.as_deref(), raw.lng.as_deref())?
        .ok_or_else(|| AppError::BadRequest("lat et lng sont obligatoires".into()))?;
    let radius_km = parse_radius(raw.radius_km.as_deref())?;
    let date = parse_date(raw.date.as_deref(), paris_today())?;
    let version = parse_version(raw.version.as_deref())?.map(|v| v.as_sql());
    let after = after_bound(date, parse_after(raw.after.as_deref())?);
    let limit = i64::from(parse_limit(raw.limit.as_deref())?);
    let cards = parse_cards(raw.cards.as_deref(), &state.pool).await?;
    let favorites = parse_cinemas(raw.cinemas.as_deref())?;
    let date = date.format(DATE_FORMAT).to_string();

    let mut nearby = match &favorites {
        Some(ids) => cinemas_by_ids(&state.pool, center, ids, cards.as_deref()).await?,
        None => cinemas_within(&state.pool, center, radius_km, cards.as_deref()).await?,
    };
    if nearby.is_empty() {
        return Ok(Json(ShowtimesResponse {
            date,
            showtimes: Vec::new(),
            cinemas: Vec::new(),
            movies: Vec::new(),
        }));
    }
    let ids: Vec<&str> = nearby.iter().map(|c| c.summary.id.as_str()).collect();
    let ids = serde_json::to_string(&ids)?;

    let mut rows = sqlx::query_as!(
        ShowtimeRow,
        r#"SELECT s.id AS "id!: String", s.cinema_id AS "cinema_id!: String",
                  s.movie_id AS "movie_id!: i64", s.starts_at AS "starts_at!: String",
                  s.version AS "version!: String",
                  s.formats AS "formats!: SqlJson<Vec<String>>",
                  s.booking_url AS "booking_url?: String"
           FROM showtimes s
           WHERE s.date = ?1
             AND s.cinema_id IN (SELECT value FROM json_each(?2))
             AND (?3 IS NULL OR s.version = ?3 OR (?3 = 'VO' AND s.version = 'VOST'))
             AND (?4 IS NULL OR s.starts_at >= ?4)
           ORDER BY s.starts_at, s.id
           LIMIT ?5"#,
        date,
        ids,
        version,
        after,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;

    // À heure égale, le cinéma le plus proche d'abord : `nearby` est trié par distance.
    let rank: HashMap<&str, usize> = nearby
        .iter()
        .enumerate()
        .map(|(i, c)| (c.summary.id.as_str(), i))
        .collect();
    rows.sort_by(|a, b| {
        a.starts_at
            .cmp(&b.starts_at)
            .then_with(|| rank[a.cinema_id.as_str()].cmp(&rank[b.cinema_id.as_str()]))
    });

    let cited: HashSet<&str> = rows.iter().map(|r| r.cinema_id.as_str()).collect();
    let mut movie_ids: Vec<i64> = rows.iter().map(|r| r.movie_id).collect();
    movie_ids.sort_unstable();
    movie_ids.dedup();
    nearby.retain(|c| cited.contains(c.summary.id.as_str()));

    let movie_ids = serde_json::to_string(&movie_ids)?;
    let movies: Vec<MovieSummary> = sqlx::query_as!(
        MovieSummaryRow,
        r#"SELECT id AS "id!: i64", title AS "title!: String", poster_url AS "poster_url?: String",
                  genres AS "genres!: SqlJson<Vec<String>>",
                  runtime_min AS "runtime_min?: i64", release_date AS "release_date?: String"
           FROM movies
           WHERE id IN (SELECT value FROM json_each(?1))
           ORDER BY title_search"#,
        movie_ids,
    )
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(MovieSummary::from)
    .collect();

    let showtimes = rows
        .into_iter()
        .map(|row| NearbyShowtime {
            showtime: Showtime {
                id: row.id,
                starts_at: row.starts_at,
                version: row.version,
                formats: row.formats.0,
                booking_url: row.booking_url,
            },
            cinema_id: row.cinema_id,
            movie_id: row.movie_id,
        })
        .collect();

    Ok(Json(ShowtimesResponse {
        date,
        showtimes,
        cinemas: nearby.into_iter().map(|c| c.summary).collect(),
        movies,
    }))
}
