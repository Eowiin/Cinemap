use axum::{Json, extract::State};

use super::AppState;
use super::error::ApiResult;
use super::types::Meta;
use crate::time::{DATE_FORMAT, paris_today};

/// État des données. Sert aussi de health check : quelques requêtes légères.
pub async fn meta(State(state): State<AppState>) -> ApiResult<Json<Meta>> {
    let today = paris_today().format(DATE_FORMAT).to_string();

    // Seuls les runs complets comptent ; finished_at est en UTC au format SQLite.
    let last_scrape_at = sqlx::query_scalar!(
        r#"SELECT strftime('%Y-%m-%dT%H:%M:%SZ', finished_at) AS "at!: String"
           FROM scrape_runs
           WHERE kind = 'showtimes' AND finished_at IS NOT NULL
           ORDER BY finished_at DESC
           LIMIT 1"#
    )
    .fetch_optional(&state.pool)
    .await?;

    let dates_available = sqlx::query_scalar!(
        r#"SELECT DISTINCT s.date AS "date!: String"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE s.date >= ?1
           ORDER BY s.date"#,
        today,
    )
    .fetch_all(&state.pool)
    .await?;

    let cinema_count = sqlx::query_scalar!(r#"SELECT count(*) AS "n!: i64" FROM visible_cinemas"#)
        .fetch_one(&state.pool)
        .await?;

    let movie_count = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT s.movie_id) AS "n!: i64"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE s.date >= ?1"#,
        today,
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(Meta {
        today,
        last_scrape_at,
        dates_available,
        cinema_count,
        movie_count,
    }))
}
