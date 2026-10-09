use axum::{
    Json,
    extract::{Query, State},
};
use sqlx::types::Json as SqlJson;

use super::AppState;
use super::cinemas::CinemaRow;
use super::error::ApiResult;
use super::movies::MovieSummaryRow;
use super::params::{RawQuery, parse_search_query};
use super::types::SearchResponse;
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

pub async fn search(
    State(state): State<AppState>,
    Query(raw): Query<RawQuery>,
) -> ApiResult<Json<SearchResponse>> {
    let q = parse_search_query(raw.q.as_deref())?;
    let pattern = like_pattern(&q);
    let today = paris_today().format(DATE_FORMAT).to_string();

    // Films ayant au moins une séance à venir dans un cinéma visible, les plus diffusés d'abord.
    let movies = sqlx::query_as!(
        MovieSummaryRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  m.poster_url AS "poster_url?: String",
                  m.genres AS "genres!: SqlJson<Vec<String>>",
                  m.runtime_min AS "runtime_min?: i64", m.release_date AS "release_date?: String"
           FROM movies m
           JOIN showtimes s ON s.movie_id = m.id
           JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE s.date >= ?1
             AND m.title_search LIKE ?2 ESCAPE '\'
           GROUP BY m.id
           ORDER BY count(*) DESC, m.title_search
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

    Ok(Json(SearchResponse {
        movies: movies.into_iter().map(Into::into).collect(),
        // Pas de position dans /api/search : distance_km = null.
        cinemas: cinemas.into_iter().map(|c| c.into_summary(None)).collect(),
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
