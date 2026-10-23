use axum::{
    Json,
    extract::{Query, State},
};
use sqlx::types::Json as SqlJson;

use super::AppState;
use super::cinemas::CinemaRow;
use super::error::ApiResult;
use super::params::{RawQuery, parse_search_query};
use super::types::{MovieSummary, SearchMovie, SearchResponse};
use crate::time::{DATE_FORMAT, paris_today};

/// Motif `LIKE … ESCAPE '\'` qui cherche `q` tel quel : `%` et `_` tapés par
/// l'utilisateur ne sont pas des jokers.
fn like_pattern(q: &str) -> String {
    let mut pattern = String::with_capacity(q.len() + 2);
    pattern.push('%');
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

struct SearchMovieRow {
    id: i64,
    title: String,
    poster_url: Option<String>,
    genres: SqlJson<Vec<String>>,
    runtime_min: Option<i64>,
    release_date: Option<String>,
    showtime_count: i64,
}

pub async fn search(
    State(state): State<AppState>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<SearchResponse>> {
    let q = parse_search_query(raw.q.as_deref())?;
    let pattern = like_pattern(&q);
    let today = paris_today().format(DATE_FORMAT).to_string();

    // Films ayant au moins une séance à venir dans un cinéma visible, les plus diffusés d'abord.
    // Les films qui ne sont plus à l'affiche restent trouvables (leur fiche dit où les
    // voir), après ceux qui le sont, les plus récemment partis d'abord.
    let movies = sqlx::query_as!(
        SearchMovieRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  m.poster_url AS "poster_url?: String",
                  m.genres AS "genres!: SqlJson<Vec<String>>",
                  m.runtime_min AS "runtime_min?: i64", m.release_date AS "release_date?: String",
                  count(c.id) AS "showtime_count!: i64"
           FROM movies m
           LEFT JOIN showtimes s ON s.movie_id = m.id AND s.date >= ?1
           LEFT JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE m.title_search LIKE ?2 ESCAPE '\'
           GROUP BY m.id
           ORDER BY count(c.id) DESC, m.last_showtime_date DESC, m.title_search
           LIMIT 8"#,
        today,
        pattern,
    )
    .fetch_all(&state.pool)
    .await?;

    let cinemas = sqlx::query_as!(
        CinemaRow,
        r#"SELECT c.id AS "id!: String", c.name AS "name!: String", c.city AS "city?: String",
                  c.lat AS "lat!: f64", c.lng AS "lng!: f64",
                  c.art_et_essai AS "art_et_essai!: bool",
                  (SELECT json_group_array(card_id) FROM cinema_cards WHERE cinema_id = c.id)
                    AS "cards!: SqlJson<Vec<String>>"
           FROM visible_cinemas c
           WHERE c.name_search LIKE ?1 ESCAPE '\' OR c.city_search LIKE ?1 ESCAPE '\'
           ORDER BY c.name_search
           LIMIT 8"#,
        pattern,
    )
    .fetch_all(&state.pool)
    .await?;

    let index = state.cities.get(&state.pool, &state.departments).await?;
    let cities = index
        .search(&q, 5)
        .into_iter()
        .map(|c| c.summary())
        .collect();

    Ok(Json(SearchResponse {
        movies: movies
            .into_iter()
            .map(|row| SearchMovie {
                showing: row.showtime_count > 0,
                movie: MovieSummary {
                    id: row.id,
                    title: row.title,
                    poster_url: row.poster_url,
                    genres: row.genres.0,
                    runtime_min: row.runtime_min,
                    release_date: row.release_date,
                },
            })
            .collect(),
        // Pas de position dans /api/search : distance_km = null.
        cinemas: cinemas.into_iter().map(|c| c.into_summary(None)).collect(),
        cities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("cine cite"), "%cine cite%");
        assert_eq!(like_pattern("100%"), r"%100\%%");
        assert_eq!(like_pattern("a_b"), r"%a\_b%");
        assert_eq!(like_pattern(r"a\b"), r"%a\\b%");
    }
}
