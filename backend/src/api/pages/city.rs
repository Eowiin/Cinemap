//! `/ville/{slug}` : les cinémas d'une ville, ceux des environs et les films du jour.

use std::fmt::Write;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use serde_json::json;

use super::html::{Head, SITE_NAME, SITE_URL, escape, truncate};
use super::{cinema_path, cinemas_count, movie_path, not_found_page, page};
use crate::api::cities::fetch_city;
use crate::api::geo::Position;
use crate::api::movies::cinemas_within;
use crate::api::types::{CinemaSummary, City};
use crate::api::{ApiResult, AppError, AppState};
use crate::time::{DATE_FORMAT, paris_today};

/// « Autour de Chelles » : les cinémas à moins de 10 km, hors de la ville.
const NEARBY_KM: f64 = 10.0;
const MAX_NEARBY: usize = 20;
const MAX_MOVIES: i64 = 30;

pub fn city_path(slug: &str) -> String {
    format!("/ville/{slug}")
}

/// Même règle dans le front (`cityTitle`).
pub fn city_title(label: &str, cinema_count: i64) -> String {
    if cinema_count > 1 {
        format!("Cinéma à {label} : séances et horaires des {cinema_count} cinémas | {SITE_NAME}")
    } else {
        format!("Cinéma à {label} : séances et horaires | {SITE_NAME}")
    }
}

struct CityMovieRow {
    id: i64,
    title: String,
    cinema_count: i64,
}

pub async fn city(State(state): State<AppState>, Path(slug): Path<String>) -> ApiResult<Response> {
    let city = match fetch_city(&state, &slug).await {
        Ok(city) => city,
        Err(AppError::NotFound(_)) => return Ok(not_found_page(&state)),
        Err(e) => return Err(e),
    };
    let center = Position {
        lat: city.lat,
        lng: city.lng,
    };
    let nearby: Vec<CinemaSummary> = cinemas_within(&state.pool, center, NEARBY_KM, None)
        .await?
        .into_iter()
        .map(|c| c.summary)
        .filter(|c| !city.cinemas.iter().any(|own| own.id == c.id))
        .take(MAX_NEARBY)
        .collect();

    let ids: Vec<&str> = city
        .cinemas
        .iter()
        .chain(&nearby)
        .map(|c| c.id.as_str())
        .collect();
    let ids = serde_json::to_string(&ids)?;
    let today = paris_today().format(DATE_FORMAT).to_string();
    let movies = sqlx::query_as!(
        CityMovieRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  count(DISTINCT s.cinema_id) AS "cinema_count!: i64"
           FROM showtimes s
           JOIN movies m ON m.id = s.movie_id
           WHERE s.date = ?1
             AND s.cinema_id IN (SELECT value FROM json_each(?2))
           GROUP BY m.id
           ORDER BY count(DISTINCT s.cinema_id) DESC, count(*) DESC, m.title_search
           LIMIT ?3"#,
        today,
        ids,
        MAX_MOVIES,
    )
    .fetch_all(&state.pool)
    .await?;

    // Une ville dont aucun cinéma n'a de séance à venir (hors de la zone scrapée) reste
    // consultable mais n'est pas proposée aux moteurs : une page sans séance les décevrait.
    let own_ids: Vec<&str> = city.cinemas.iter().map(|c| c.id.as_str()).collect();
    let own_ids = serde_json::to_string(&own_ids)?;
    let has_showtimes = sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM showtimes
               WHERE cinema_id IN (SELECT value FROM json_each(?1)) AND date >= ?2
           ) AS "has!: bool""#,
        own_ids,
        today,
    )
    .fetch_one(&state.pool)
    .await?;

    let label = &city.summary.label;
    let path = city_path(&city.summary.slug);
    let title = city_title(label, city.summary.cinema_count);
    let description = city_description(&city, nearby.len());
    let body = city_body(&city, &nearby, &movies);
    let url = format!("{SITE_URL}{path}");
    let theaters: Vec<_> = city
        .cinemas
        .iter()
        .enumerate()
        .map(|(i, c)| {
            json!({
                "@type": "ListItem",
                "position": i + 1,
                "item": {
                    "@type": "MovieTheater",
                    "name": c.name,
                    "url": format!("{SITE_URL}{}", cinema_path(&c.id, &c.name, c.city.as_deref())),
                },
            })
        })
        .collect();
    let head = Head {
        title: &title,
        description: &description,
        path: &path,
        image: None,
        og_type: "website",
        noindex: !has_showtimes,
        json_ld: Some(json!({
            "@context": "https://schema.org",
            "@type": "ItemList",
            "name": format!("Cinémas à {label}"),
            "url": url,
            "itemListElement": theaters,
        })),
    };
    Ok(page(&state, StatusCode::OK, &head, &body))
}

fn city_description(city: &City, nearby: usize) -> String {
    let label = &city.summary.label;
    let around = if nearby > 0 { " et des environs" } else { "" };
    let text = match city.cinemas.as_slice() {
        [only] => format!(
            "Séances et horaires du cinéma de {label} ({}){around} : films à l'affiche aujourd'hui, VF et VO, sur une carte.",
            only.name
        ),
        _ => format!(
            "Séances et horaires des {} de {label}{around} : films à l'affiche aujourd'hui, VF et VO, sur une carte.",
            cinemas_count(city.summary.cinema_count)
        ),
    };
    truncate(&text, 160)
}

fn cinema_items(body: &mut String, cinemas: &[CinemaSummary], with_city: bool) {
    body.push_str("<ul>");
    for c in cinemas {
        let _ = write!(
            body,
            "<li><a href=\"{}\">{}</a>",
            escape(&cinema_path(&c.id, &c.name, c.city.as_deref())),
            escape(&c.name),
        );
        let mut notes = Vec::new();
        if with_city && let Some(city) = &c.city {
            notes.push(city.clone());
        }
        if c.art_et_essai {
            notes.push("Art et Essai".to_owned());
        }
        if !notes.is_empty() {
            let _ = write!(body, " ({})", escape(&notes.join(", ")));
        }
        body.push_str("</li>");
    }
    body.push_str("</ul>");
}

fn city_body(city: &City, nearby: &[CinemaSummary], movies: &[CityMovieRow]) -> String {
    let label = escape(&city.summary.label);
    let mut body = format!(
        "<div class=\"ssr\"><h1>Cinéma à {label}</h1>\
         <p>Les films à l'affiche et leurs horaires dans les cinémas de {label}{}, sur une \
         carte : choisissez un film ou une salle, puis composez votre programme.</p>",
        if nearby.is_empty() {
            ""
        } else {
            " et des environs"
        }
    );
    let _ = write!(
        body,
        "<h2>{} à {label}</h2>",
        if city.cinemas.len() > 1 {
            format!("Les {} cinémas", city.cinemas.len())
        } else {
            "Le cinéma".to_owned()
        }
    );
    cinema_items(&mut body, &city.cinemas, false);
    if !nearby.is_empty() {
        let _ = write!(body, "<h2>Autour de {label}</h2>");
        cinema_items(&mut body, nearby, true);
    }
    if !movies.is_empty() {
        body.push_str("<h2>À l'affiche aujourd'hui</h2><ul>");
        for movie in movies {
            let _ = write!(
                body,
                "<li><a href=\"{}\">{}</a> : dans {}</li>",
                escape(&movie_path(movie.id, &movie.title)),
                escape(&movie.title),
                cinemas_count(movie.cinema_count),
            );
        }
        body.push_str("</ul>");
    }
    body.push_str("</div>");
    body
}
