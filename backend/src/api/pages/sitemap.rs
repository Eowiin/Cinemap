use std::fmt::Write;

use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use super::html::{SITE_URL, escape};
use super::{cinema_path, city_path, movie_path};
use crate::api::{ApiResult, AppState};
use crate::time::{DATE_FORMAT, paris_today};

struct CinemaRow {
    id: String,
    name: String,
    city: Option<String>,
    department: Option<String>,
}

struct MovieRow {
    id: i64,
    title: String,
}

/// Pages qui ont des séances à venir : accueil, villes (au moins un de leurs cinémas),
/// cinémas et films. Hors de la zone scrapée, les pages existent mais restent en dehors
/// (et en `noindex`) : des milliers de pages sans séance nuiraient à tout le site.
/// Construit à la demande : deux requêtes et l'index des villes.
pub async fn sitemap(State(state): State<AppState>) -> ApiResult<Response> {
    let today = paris_today().format(DATE_FORMAT).to_string();
    let cinemas = sqlx::query_as!(
        CinemaRow,
        r#"SELECT c.id AS "id!: String", c.name AS "name!: String", c.city AS "city?: String",
                  c.department AS "department?: String"
           FROM visible_cinemas c
           WHERE EXISTS (SELECT 1 FROM showtimes s WHERE s.cinema_id = c.id AND s.date >= ?1)
           ORDER BY c.id"#,
        today,
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

    // Les villes dont au moins un cinéma a des séances à venir, dans l'ordre de l'index.
    let index = state.cities.get(&state.pool, &state.departments).await?;
    let showing: std::collections::HashSet<&str> = cinemas
        .iter()
        .filter_map(|c| index.slug_of(c.city.as_deref()?, c.department.as_deref()))
        .collect();
    let cities: Vec<&str> = index
        .cities
        .iter()
        .map(|c| c.slug.as_str())
        .filter(|slug| showing.contains(slug))
        .collect();

    // Le contenu de toutes les pages vient des séances : il change à chaque scrape. Dire
    // à Google quand (`lastmod`) l'aide à revenir lire les pages au bon moment.
    let lastmod = sqlx::query_scalar!(
        r#"SELECT date(finished_at) AS "date!: String"
           FROM scrape_runs
           WHERE kind = 'showtimes' AND finished_at IS NOT NULL
           ORDER BY finished_at DESC
           LIMIT 1"#
    )
    .fetch_optional(&state.pool)
    .await?
    .map(|date| format!("<lastmod>{date}</lastmod>"))
    .unwrap_or_default();

    let capacity = cinemas.len() + movies.len() + cities.len() + 1;
    let mut xml = String::with_capacity(100 * capacity);
    xml.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    let paths = std::iter::once("/".to_owned())
        .chain(cities.iter().map(|slug| city_path(slug)))
        .chain(movies.iter().map(|m| movie_path(m.id, &m.title)))
        .chain(
            cinemas
                .iter()
                .map(|c| cinema_path(&c.id, &c.name, c.city.as_deref())),
        );
    for path in paths {
        let _ = writeln!(
            xml,
            "<url><loc>{}</loc>{lastmod}</url>",
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
