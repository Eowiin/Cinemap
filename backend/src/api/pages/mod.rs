//! Pages HTML servies aux robots comme aux visiteurs : `index.html` du front, avec le
//! titre, la description, l'URL canonique, les données structurées (schema.org) et un
//! contenu lisible sans JavaScript. L'app Svelte remplace ce contenu en démarrant.

use std::fmt::Write;

use axum::{
    extract::{Path, State},
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Europe::Paris;
use serde_json::{Value, json};

use super::cinemas::{fetch_cinema, programme};
use super::cities::CityIndex;
use super::movies::fetch_movie;
use super::types::{Cinema, Movie, MovieShowtimes};
use super::{ApiResult, AppError, AppState};
use crate::text::{cinema_slug, locality, slugify};
use crate::time::{DATE_FORMAT, paris_today};

mod city;
mod html;
mod sitemap;
mod template;

pub use city::city;
use city::city_path;
use html::{Head, SITE_NAME, SITE_URL, escape, french_date, french_day, runtime, truncate};
pub use sitemap::sitemap;
pub use template::PageTemplate;

/// Au-delà, la liste des cinémas d'un film s'arrête (les plus gros films passent dans
/// ~1 000 salles : la page resterait lourde pour rien, le sitemap couvre le reste).
const MAX_CINEMAS_LISTED: i64 = 300;
const HOME_MOVIES: i64 = 40;
/// Villes listées dans le contenu lisible de l'accueil (les plus fournies).
const HOME_CITIES: usize = 24;

/// `/film/{id}-{slug}` : l'id fait foi, le slug n'est là que pour être lu.
pub fn movie_path(id: i64, title: &str) -> String {
    with_slug(&format!("/film/{id}"), &slugify(title))
}

pub fn cinema_path(id: &str, name: &str, city: Option<&str>) -> String {
    with_slug(&format!("/cinema/{id}"), &cinema_slug(name, city))
}

fn with_slug(base: &str, slug: &str) -> String {
    if slug.is_empty() {
        base.to_owned()
    } else {
        format!("{base}-{slug}")
    }
}

/// Titre d'un cinéma : la ville n'est ajoutée que si le nom ne la dit pas déjà.
/// Même règle dans le front (`cinemaPageTitle`).
pub fn cinema_title(name: &str, city: Option<&str>, postal_code: Option<&str>) -> String {
    let place = city.map(|city| locality(city, postal_code));
    let named = |city: &str| {
        let city = slugify(city);
        format!("-{}-", slugify(name)).contains(&format!("-{city}-"))
    };
    match (place, city) {
        (Some(place), Some(city)) if place != city || !named(city) => {
            format!("{name} ({place}) : programme et horaires | {SITE_NAME}")
        }
        _ => format!("{name} : programme et horaires | {SITE_NAME}"),
    }
}

pub fn movie_title(title: &str) -> String {
    format!("{title} : séances et horaires | {SITE_NAME}")
}

pub const HOME_TITLE: &str = "Cinemap : séances de cinéma près de chez vous";
const HOME_DESCRIPTION: &str = "Les horaires de cinéma près de chez vous, sur une carte : \
    plus de 3 000 cinémas en France, films à l'affiche, séances en VF et en VO, \
    cartes UGC Illimité et Pathé CinéPass.";

fn html_response(status: StatusCode, html: String) -> Response {
    let mut response = (status, html).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Revalidé à chaque fois : il pointe vers les fichiers de la release en cours.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// Page complète, ou 404 JSON si le front n'a pas été trouvé au démarrage (dev sans build).
fn page(state: &AppState, status: StatusCode, head: &Head, body: &str) -> Response {
    match &state.pages {
        Some(template) => html_response(status, template.render(&head.render(), body)),
        None => AppError::NotFound("Front absent : pages HTML indisponibles").into_response(),
    }
}

fn redirect(path: &str, uri: &Uri) -> Response {
    let location = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_owned(),
    };
    match HeaderValue::from_str(&location) {
        Ok(location) => (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, location)],
        )
            .into_response(),
        Err(_) => AppError::NotFound("Page introuvable").into_response(),
    }
}

/// Id au début d'un segment `{id}-{slug}` (les ids n'ont jamais de tiret).
fn split_segment(segment: &str) -> &str {
    segment.split_once('-').map_or(segment, |(id, _)| id)
}

pub fn not_found_page(state: &AppState) -> Response {
    let head = Head {
        title: "Page introuvable | Cinemap",
        description: "Ce lien ne mène nulle part.",
        path: "/",
        image: None,
        og_type: "website",
        noindex: true,
        json_ld: None,
    };
    let body = "<div class=\"ssr\"><h1>Page introuvable</h1>\
                <p><a href=\"/\">Voir les films à l'affiche</a></p></div>";
    page(state, StatusCode::NOT_FOUND, &head, body)
}

/// Route inconnue : JSON sous `/api/`, page 404 ailleurs.
pub async fn fallback(State(state): State<AppState>, uri: Uri) -> Response {
    if uri.path().starts_with("/api/") || state.pages.is_none() {
        AppError::NotFound("Route introuvable").into_response()
    } else {
        not_found_page(&state)
    }
}

struct HomeMovieRow {
    id: i64,
    title: String,
    cinema_count: i64,
}

pub async fn home(State(state): State<AppState>) -> ApiResult<Response> {
    let today = paris_today().format(DATE_FORMAT).to_string();
    let movies = sqlx::query_as!(
        HomeMovieRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  count(DISTINCT s.cinema_id) AS "cinema_count!: i64"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           JOIN movies m ON m.id = s.movie_id
           WHERE s.date = ?1
           GROUP BY m.id
           ORDER BY count(DISTINCT s.cinema_id) DESC, m.title_search
           LIMIT ?2"#,
        today,
        HOME_MOVIES,
    )
    .fetch_all(&state.pool)
    .await?;

    let mut body = String::from(
        "<div class=\"ssr\"><h1>Séances de cinéma près de chez vous</h1>\
         <p>Cinemap montre sur une carte les films à l'affiche et leurs horaires dans plus \
         de 3 000 cinémas en France : multiplexes, salles Art et Essai, cinémas qui acceptent \
         UGC Illimité ou Pathé CinéPass. Choisissez un film, un cinéma ou une heure, puis \
         composez votre programme de la journée.</p>",
    );
    if !movies.is_empty() {
        body.push_str("<h2>À l'affiche aujourd'hui</h2><ul>");
        for movie in &movies {
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
    // Liens vers les grandes villes : invisibles une fois l'app démarrée, ils aident les
    // moteurs à trouver les pages ville.
    let index = state.cities.get(&state.pool, &state.departments).await?;
    if !index.cities.is_empty() {
        body.push_str("<h2>Cinémas par ville</h2><ul>");
        for city in index.cities.iter().take(HOME_CITIES) {
            let _ = write!(
                body,
                "<li><a href=\"{}\">Cinéma à {}</a></li>",
                escape(&city_path(&city.slug)),
                escape(&city.label),
            );
        }
        body.push_str("</ul>");
    }
    body.push_str("</div>");

    let head = Head {
        title: HOME_TITLE,
        description: HOME_DESCRIPTION,
        path: "/",
        image: None,
        og_type: "website",
        noindex: false,
        json_ld: Some(json!({
            "@context": "https://schema.org",
            "@type": "WebSite",
            "name": SITE_NAME,
            "url": format!("{SITE_URL}/"),
            "inLanguage": "fr-FR",
        })),
    };
    Ok(page(&state, StatusCode::OK, &head, &body))
}

fn cinemas_count(n: i64) -> String {
    if n > 1 {
        format!("{n} cinémas")
    } else {
        format!("{n} cinéma")
    }
}

struct ShowingCinemaRow {
    id: String,
    name: String,
    city: Option<String>,
    department: Option<String>,
}

/// Les cinémas d'un film, ville par ville, avec la page de la ville quand elle existe.
struct CityGroup<'a> {
    city: Option<&'a str>,
    slug: Option<String>,
    cinemas: Vec<&'a ShowingCinemaRow>,
}

fn group_by_city<'a>(rows: &'a [ShowingCinemaRow], index: &CityIndex) -> Vec<CityGroup<'a>> {
    let mut groups: Vec<CityGroup<'a>> = Vec::new();
    for row in rows {
        // Lignes triées par ville : un groupe = des lignes consécutives.
        match groups.last_mut() {
            Some(group)
                if group.city == row.city.as_deref()
                    && group.cinemas[0].department == row.department =>
            {
                group.cinemas.push(row);
            }
            _ => groups.push(CityGroup {
                city: row.city.as_deref(),
                slug: row
                    .city
                    .as_deref()
                    .and_then(|city| index.slug_of(city, row.department.as_deref()))
                    .map(str::to_owned),
                cinemas: vec![row],
            }),
        }
    }
    groups
}

pub async fn movie(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    uri: Uri,
) -> ApiResult<Response> {
    let Ok(id) = split_segment(&segment).parse::<i64>() else {
        return Ok(not_found_page(&state));
    };
    let movie = match fetch_movie(&state.pool, id).await {
        Ok(movie) => movie,
        Err(AppError::NotFound(_)) => return Ok(not_found_page(&state)),
        Err(e) => return Err(e),
    };
    let path = movie_path(id, &movie.summary.title);
    if uri.path() != path {
        return Ok(redirect(&path, &uri));
    }

    let today = paris_today().format(DATE_FORMAT).to_string();
    let cinema_count = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT s.cinema_id) AS "n!: i64"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE s.movie_id = ?1 AND s.date >= ?2"#,
        id,
        today,
    )
    .fetch_one(&state.pool)
    .await?;
    let cinemas = sqlx::query_as!(
        ShowingCinemaRow,
        r#"SELECT DISTINCT c.id AS "id!: String", c.name AS "name!: String",
                  c.city AS "city?: String", c.department AS "department?: String"
           FROM showtimes s
           JOIN visible_cinemas c ON c.id = s.cinema_id
           WHERE s.movie_id = ?1 AND s.date >= ?2
           ORDER BY c.city, c.department, c.name_search
           LIMIT ?3"#,
        id,
        today,
        MAX_CINEMAS_LISTED,
    )
    .fetch_all(&state.pool)
    .await?;

    let title = movie_title(&movie.summary.title);
    let description = movie_description(&movie, cinema_count);
    let index = state.cities.get(&state.pool, &state.departments).await?;
    let body = movie_body(&movie, cinema_count, &group_by_city(&cinemas, &index));
    let head = Head {
        title: &title,
        description: &description,
        path: &path,
        image: movie.summary.poster_url.as_deref(),
        og_type: "video.movie",
        // Sans séance à venir, la page n'aide personne : consultable, mais pas indexée.
        noindex: cinema_count == 0,
        json_ld: Some(movie_json_ld(&movie, &path)),
    };
    Ok(page(&state, StatusCode::OK, &head, &body))
}

/// « Genre · 1 h 55 · sortie le … · de Réalisateur » (morceaux absents omis).
fn movie_facts(movie: &Movie) -> Vec<String> {
    let mut facts = Vec::new();
    if !movie.summary.genres.is_empty() {
        facts.push(movie.summary.genres.join(", "));
    }
    if let Some(minutes) = movie.summary.runtime_min.filter(|m| *m > 0) {
        facts.push(runtime(minutes));
    }
    if let Some(date) = &movie.summary.release_date {
        facts.push(format!("sortie le {}", french_date(date)));
    }
    if !movie.directors.is_empty() {
        facts.push(format!("de {}", movie.directors.join(", ")));
    }
    facts
}

fn movie_description(movie: &Movie, cinema_count: i64) -> String {
    let title = &movie.summary.title;
    let mut text = if cinema_count > 0 {
        format!(
            "{title} : séances et horaires dans {} en France, en VF et en VO.",
            cinemas_count(cinema_count)
        )
    } else {
        format!("{title} : aucune séance programmée en ce moment.")
    };
    match &movie.synopsis {
        Some(synopsis) => {
            text.push(' ');
            text.push_str(synopsis);
        }
        None => {
            let facts = movie_facts(movie);
            if !facts.is_empty() {
                let _ = write!(text, " {}.", facts.join(", "));
            }
        }
    }
    truncate(&text, 160)
}

fn movie_body(movie: &Movie, cinema_count: i64, cities: &[CityGroup]) -> String {
    let title = escape(&movie.summary.title);
    let mut body = format!("<div class=\"ssr\"><h1>{title}</h1>");
    let facts = movie_facts(movie);
    if !facts.is_empty() {
        let _ = write!(body, "<p>{}</p>", escape(&facts.join(" · ")));
    }
    if let Some(synopsis) = &movie.synopsis {
        let _ = write!(body, "<p>{}</p>", escape(synopsis));
    }
    if !movie.cast.is_empty() {
        let names: Vec<&str> = movie.cast.iter().take(6).map(|p| p.name.as_str()).collect();
        let _ = write!(body, "<p>Avec {}.</p>", escape(&names.join(", ")));
    }
    let _ = write!(body, "<h2>Où voir {title} ?</h2>");
    if cities.is_empty() {
        body.push_str("<p>Aucune séance programmée en ce moment.</p>");
    } else {
        let _ = write!(
            body,
            "<p>À l'affiche dans {} cette semaine.</p>",
            cinemas_count(cinema_count)
        );
        for group in cities {
            match (group.city, &group.slug) {
                (Some(city), Some(slug)) => {
                    let _ = write!(
                        body,
                        "<h3><a href=\"{}\">{}</a></h3>",
                        escape(&city_path(slug)),
                        escape(city)
                    );
                }
                (Some(city), None) => {
                    let _ = write!(body, "<h3>{}</h3>", escape(city));
                }
                (None, _) => {}
            }
            body.push_str("<ul>");
            for cinema in &group.cinemas {
                let _ = write!(
                    body,
                    "<li><a href=\"{}\">{}</a></li>",
                    escape(&cinema_path(
                        &cinema.id,
                        &cinema.name,
                        cinema.city.as_deref()
                    )),
                    escape(&cinema.name),
                );
            }
            body.push_str("</ul>");
        }
    }
    body.push_str("</div>");
    body
}

fn movie_json_ld(movie: &Movie, path: &str) -> Value {
    let mut ld = json!({
        "@context": "https://schema.org",
        "@type": "Movie",
        "name": movie.summary.title,
        "url": format!("{SITE_URL}{path}"),
    });
    let fields = ld.as_object_mut().expect("objet JSON");
    if let Some(poster) = &movie.summary.poster_url {
        fields.insert("image".into(), json!(poster));
    }
    if let Some(synopsis) = &movie.synopsis {
        fields.insert("description".into(), json!(synopsis));
    }
    if !movie.summary.genres.is_empty() {
        fields.insert("genre".into(), json!(movie.summary.genres));
    }
    if let Some(minutes) = movie.summary.runtime_min.filter(|m| *m > 0) {
        fields.insert("duration".into(), json!(format!("PT{minutes}M")));
    }
    if let Some(date) = &movie.summary.release_date {
        fields.insert("datePublished".into(), json!(date));
    }
    if let Some(original) = movie
        .original_title
        .as_ref()
        .filter(|t| **t != movie.summary.title)
    {
        fields.insert("alternateName".into(), json!(original));
    }
    let people = |names: &mut dyn Iterator<Item = &String>| -> Value {
        names
            .map(|name| json!({ "@type": "Person", "name": name }))
            .collect()
    };
    if !movie.directors.is_empty() {
        fields.insert("director".into(), people(&mut movie.directors.iter()));
    }
    if !movie.cast.is_empty() {
        fields.insert(
            "actor".into(),
            people(&mut movie.cast.iter().take(10).map(|p| &p.name)),
        );
    }
    ld
}

pub async fn cinema(
    State(state): State<AppState>,
    Path(segment): Path<String>,
    uri: Uri,
) -> ApiResult<Response> {
    let id = split_segment(&segment);
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Ok(not_found_page(&state));
    }
    let cinema = match fetch_cinema(&state, id).await {
        Ok(cinema) => cinema,
        Err(AppError::NotFound(_)) => return Ok(not_found_page(&state)),
        Err(e) => return Err(e),
    };
    let summary = &cinema.summary;
    let path = cinema_path(&summary.id, &summary.name, summary.city.as_deref());
    if uri.path() != path {
        return Ok(redirect(&path, &uri));
    }

    let today = paris_today().format(DATE_FORMAT).to_string();
    let dates = sqlx::query_scalar!(
        r#"SELECT DISTINCT date AS "date!: String"
           FROM showtimes
           WHERE cinema_id = ?1 AND date >= ?2
           ORDER BY date"#,
        summary.id,
        today,
    )
    .fetch_all(&state.pool)
    .await?;
    let movie_count = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT movie_id) AS "n!: i64"
           FROM showtimes
           WHERE cinema_id = ?1 AND date >= ?2"#,
        summary.id,
        today,
    )
    .fetch_one(&state.pool)
    .await?;
    // Toute la semaine publiée (7 jours au plus) : la page dit ce qu'on y verra, pas
    // seulement le jour même.
    let mut days = Vec::with_capacity(dates.len());
    for date in &dates {
        if let Ok(day) = NaiveDate::parse_from_str(date, DATE_FORMAT) {
            days.push((
                day,
                programme(&state.pool, &summary.id, date, None, None).await?,
            ));
        }
    }
    let card_names = sqlx::query_scalar!(
        r#"SELECT name AS "name!: String"
           FROM cards
           WHERE id IN (SELECT card_id FROM cinema_cards WHERE cinema_id = ?1)
           ORDER BY name"#,
        summary.id,
    )
    .fetch_all(&state.pool)
    .await?;
    let title = cinema_title(
        &summary.name,
        summary.city.as_deref(),
        cinema.postal_code.as_deref(),
    );
    let description = cinema_description(&cinema, movie_count);
    let body = cinema_body(&cinema, &card_names, &days);
    let next_day = days
        .first()
        .map_or(&[][..], |(_, movies)| movies.as_slice());
    let head = Head {
        title: &title,
        description: &description,
        path: &path,
        image: None,
        og_type: "website",
        // Hors de la zone scrapée (ou salle sans programme) : rien à proposer aux moteurs.
        noindex: movie_count == 0,
        json_ld: Some(cinema_json_ld(&cinema, &path, next_day, &paris_now())),
    };
    Ok(page(&state, StatusCode::OK, &head, &body))
}

fn cinema_place(cinema: &Cinema) -> Option<String> {
    cinema
        .summary
        .city
        .as_deref()
        .map(|city| locality(city, cinema.postal_code.as_deref()))
}

fn cinema_description(cinema: &Cinema, movie_count: i64) -> String {
    let name = &cinema.summary.name;
    let place = cinema_place(cinema)
        .map(|p| format!(" ({p})"))
        .unwrap_or_default();
    let text = match movie_count {
        0 => format!(
            "Programme du cinéma {name}{place} : aucune séance annoncée pour l'instant. Adresse et accès."
        ),
        1 => format!(
            "Programme et horaires du cinéma {name}{place} : 1 film à l'affiche cette semaine, adresse et accès."
        ),
        n => format!(
            "Programme et horaires du cinéma {name}{place} : {n} films à l'affiche cette semaine, séances en VF et en VO, adresse et accès."
        ),
    };
    truncate(&text, 160)
}

fn cinema_body(
    cinema: &Cinema,
    card_names: &[String],
    days: &[(NaiveDate, Vec<MovieShowtimes>)],
) -> String {
    let summary = &cinema.summary;
    let mut body = format!("<div class=\"ssr\"><h1>{}</h1>", escape(&summary.name));

    let mut facts = Vec::new();
    if let Some(address) = &cinema.address {
        facts.push(address.clone());
    } else if let Some(place) = cinema_place(cinema) {
        facts.push(place);
    }
    if let Some(screens) = cinema.screens.filter(|n| *n > 0) {
        facts.push(if screens > 1 {
            format!("{screens} salles")
        } else {
            "1 salle".to_owned()
        });
    }
    if summary.art_et_essai {
        facts.push("Art et Essai".to_owned());
    }
    if !card_names.is_empty() {
        facts.push(format!("accepte {}", card_names.join(" et ")));
    }
    if !facts.is_empty() {
        let _ = write!(body, "<p>{}</p>", escape(&facts.join(" · ")));
    }
    if let (Some(slug), Some(city)) = (&cinema.city_slug, &summary.city) {
        let _ = write!(
            body,
            "<p><a href=\"{}\">Tous les cinémas de {}</a></p>",
            escape(&city_path(slug)),
            escape(city),
        );
    }

    if days.is_empty() {
        body.push_str("<p>Aucune séance annoncée pour l'instant.</p>");
    }
    for (day, movies) in days {
        let _ = write!(body, "<h2>Séances du {}</h2><ul>", french_day(*day));
        for entry in movies {
            let times: Vec<String> = entry
                .showtimes
                .iter()
                .map(|s| {
                    let time = s.starts_at.get(11..16).unwrap_or(&s.starts_at);
                    format!("{} ({})", time.replace(':', "h"), s.version)
                })
                .collect();
            let _ = write!(
                body,
                "<li><a href=\"{}\">{}</a> : {}</li>",
                escape(&movie_path(entry.movie.id, &entry.movie.title)),
                escape(&entry.movie.title),
                escape(&times.join(", ")),
            );
        }
        body.push_str("</ul>");
    }
    body.push_str("</div>");
    body
}

fn paris_now() -> String {
    chrono::Utc::now()
        .with_timezone(&Paris)
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

/// `2026-10-12T20:00:00` (heure de Paris) → `2026-10-12T20:00:00+02:00`.
fn paris_rfc3339(starts_at: &str) -> Option<String> {
    let local = NaiveDateTime::parse_from_str(starts_at, "%Y-%m-%dT%H:%M:%S").ok()?;
    Paris
        .from_local_datetime(&local)
        .earliest()
        .map(|dt| dt.to_rfc3339())
}

/// `now` : heure de Paris au format de `starts_at`, pour choisir la prochaine séance.
fn cinema_json_ld(cinema: &Cinema, path: &str, movies: &[MovieShowtimes], now: &str) -> Value {
    let summary = &cinema.summary;
    let mut address = json!({ "@type": "PostalAddress", "addressCountry": "FR" });
    let fields = address.as_object_mut().expect("objet JSON");
    if let Some(street) = &cinema.address {
        // L'adresse AlloCiné finit par « 75004 Paris » : on garde la rue seule.
        let street = match &cinema.postal_code {
            Some(code) => street.split(code.as_str()).next().unwrap_or(street),
            None => street,
        };
        fields.insert(
            "streetAddress".into(),
            json!(street.trim().trim_end_matches(',')),
        );
    }
    if let Some(code) = &cinema.postal_code {
        fields.insert("postalCode".into(), json!(code));
    }
    if let Some(city) = &summary.city {
        fields.insert("addressLocality".into(), json!(city));
    }

    let mut ld = json!({
        "@context": "https://schema.org",
        "@type": "MovieTheater",
        "name": summary.name,
        "url": format!("{SITE_URL}{path}"),
        "address": address,
        "geo": { "@type": "GeoCoordinates", "latitude": summary.lat, "longitude": summary.lng },
    });
    let fields = ld.as_object_mut().expect("objet JSON");
    if let Some(screens) = cinema.screens.filter(|n| *n > 0) {
        fields.insert("screenCount".into(), json!(screens));
    }
    // Une séance par film (la prochaine) : décrire les ~150 séances d'un multiplexe
    // alourdirait la page (~45 Ko) sans rien dire de plus aux moteurs.
    let url = format!("{SITE_URL}{path}");
    fields.insert("@id".into(), json!(url));
    let place = json!({ "@id": url, "@type": "MovieTheater", "name": summary.name });
    let events: Vec<Value> = movies
        .iter()
        .filter_map(|entry| {
            let showtime = entry
                .showtimes
                .iter()
                .find(|s| s.starts_at.as_str() >= now)
                .or(entry.showtimes.first())?;
            let start = paris_rfc3339(&showtime.starts_at)?;
            let movie_url = format!(
                "{SITE_URL}{}",
                movie_path(entry.movie.id, &entry.movie.title)
            );
            Some(json!({
                "@type": "ScreeningEvent",
                "name": entry.movie.title,
                "startDate": start,
                "location": place,
                "workPresented": { "@type": "Movie", "name": entry.movie.title, "url": movie_url },
            }))
        })
        .collect();
    if !events.is_empty() {
        fields.insert("event".into(), Value::Array(events));
    }
    ld
}

pub async fn programme_page(State(state): State<AppState>) -> Response {
    let head = Head {
        title: "Mon programme · Cinemap",
        description: "Les séances que vous avez gardées, dans l'ordre de la journée.",
        path: "/programme",
        image: None,
        og_type: "website",
        noindex: true,
        json_ld: None,
    };
    page(&state, StatusCode::OK, &head, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cinema_title_adds_the_place_when_it_says_something() {
        assert_eq!(
            cinema_title("Pathé Lyon", Some("Lyon"), None),
            "Pathé Lyon : programme et horaires | Cinemap"
        );
        assert_eq!(
            cinema_title("Pathé Lyon", Some("Lyon"), Some("69002")),
            "Pathé Lyon (Lyon 2e) : programme et horaires | Cinemap"
        );
        assert_eq!(
            cinema_title("Le Rex", None, None),
            "Le Rex : programme et horaires | Cinemap"
        );
    }
}
