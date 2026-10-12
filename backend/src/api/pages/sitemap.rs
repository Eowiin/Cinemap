use std::fmt::Write;

use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use super::html::{SITE_URL, escape};
use super::{cinema_path, movie_path};
use crate::api::{ApiResult, AppState};
use crate::time::{DATE_FORMAT, paris_today};

struct CinemaRow {
    id: String,
    name: String,
    city: Option<String>,
}

struct MovieRow {
    id: i64,
    title: String,
}

/// Accueil, cinémas visibles et films qui ont encore des séances (~3 700 URLs, ~350 Ko
/// avant gzip). Construit à la demande : deux requêtes, quelques millisecondes.
pub async fn sitemap(State(state): State<AppState>) -> ApiResult<Response> {
    let today = paris_today().format(DATE_FORMAT).to_string();
    let cinemas = sqlx::query_as!(
        CinemaRow,
        r#"SELECT id AS "id!: String", name AS "name!: String", city AS "city?: String"
           FROM visible_cinemas
           ORDER BY id"#,
    )
    .fetch_all(&state.pool)
    .await?;
    let movies = sqlx::query_as!(
        MovieRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String"
           FROM movies m
           WHERE EXISTS (
               SELECT 1 FROM showtimes s
               JOIN visible_cinemas c ON c.id = s.cinema_id
               WHERE s.movie_id = m.id AND s.date >= ?1)
           ORDER BY m.id"#,
        today,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut xml = String::with_capacity(100 * (cinemas.len() + movies.len() + 1));
    xml.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    let paths = std::iter::once("/".to_owned())
        .chain(movies.iter().map(|m| movie_path(m.id, &m.title)))
        .chain(
            cinemas
                .iter()
                .map(|c| cinema_path(&c.id, &c.name, c.city.as_deref())),
        );
    for path in paths {
        let _ = writeln!(
            xml,
            "<url><loc>{}</loc></url>",
            escape(&format!("{SITE_URL}{path}"))
        );
    }
    xml.push_str("</urlset>\n");

    let mut response = xml.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok(response)
}
