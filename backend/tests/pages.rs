//! Pages HTML (référencement) : contenu, URL canonique, redirections et vraies 404,
//! sur la base de `tests/fixtures/api_seed.sql` et un gabarit minimal.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use backend::api::{AppState, PageTemplate, router};
use http_body_util::BodyExt;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

const TEMPLATE: &str = "<!doctype html><html lang=\"fr\"><head>\
    <!--seo--><title>Cinemap</title><!--/seo--></head>\
    <body><div id=\"app\"><!--app--></div></body></html>";

async fn state() -> AppState {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    backend::db::run_migrations(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("fixtures/api_seed.sql"))
        .execute(&pool)
        .await
        .unwrap();
    AppState::new(pool).with_pages(PageTemplate::parse(TEMPLATE).unwrap())
}

struct Reply {
    status: StatusCode,
    location: Option<String>,
    content_type: Option<String>,
    body: String,
}

async fn get(state: &AppState, uri: &str) -> Reply {
    let response = router(state.clone())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let header = |name| {
        response
            .headers()
            .get(name)
            .map(|v: &axum::http::HeaderValue| v.to_str().unwrap().to_owned())
    };
    let (status, location, content_type) = (
        response.status(),
        header(header::LOCATION),
        header(header::CONTENT_TYPE),
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        location,
        content_type,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

#[tokio::test]
async fn movie_page_has_metadata_and_readable_content() {
    let state = state().await;
    let reply = get(&state, "/film/1001-l-ete-dernier").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.content_type.as_deref(),
        Some("text/html; charset=utf-8")
    );
    let html = &reply.body;
    assert!(html.contains("<title>L&#39;Été dernier : séances et horaires | Cinemap</title>"));
    assert!(html.contains(
        "<link rel=\"canonical\" href=\"https://cinemap.ethansaux.fr/film/1001-l-ete-dernier\" />"
    ));
    assert!(html.contains("<meta name=\"description\" content=\"L&#39;Été dernier : séances et horaires dans 3 cinémas"));
    assert!(html.contains("\"@type\":\"Movie\""));
    assert!(html.contains("<h1>L&#39;Été dernier</h1>"));
    // Les cinémas visibles seulement, avec un lien vers leur fiche.
    assert!(
        html.contains("<a href=\"/cinema/LYON-pathe-bellecour-lyon\">Pathé Bellecour</a> (Lyon)")
    );
    assert!(html.contains("/cinema/PARIS1-ugc-cine-cite-les-halles-paris"));
    assert!(!html.contains("NOGEO") && !html.contains("OLD"));
    assert!(!html.contains("<!--seo-->") && !html.contains("<!--app-->"));
}

#[tokio::test]
async fn pages_redirect_to_their_canonical_url() {
    let state = state().await;
    for (uri, expected) in [
        ("/film/1001", "/film/1001-l-ete-dernier"),
        (
            "/film/1001-vieux-titre?date=2099-01-02",
            "/film/1001-l-ete-dernier?date=2099-01-02",
        ),
        (
            "/cinema/PARIS1",
            "/cinema/PARIS1-ugc-cine-cite-les-halles-paris",
        ),
    ] {
        let reply = get(&state, uri).await;
        assert_eq!(reply.status, StatusCode::MOVED_PERMANENTLY, "{uri}");
        assert_eq!(reply.location.as_deref(), Some(expected), "{uri}");
    }
}

#[tokio::test]
async fn cinema_page_lists_the_next_showtimes() {
    let state = state().await;
    let reply = get(&state, "/cinema/PARIS1-ugc-cine-cite-les-halles-paris").await;
    assert_eq!(reply.status, StatusCode::OK);
    let html = &reply.body;
    assert!(html.contains(
        "<title>UGC Ciné Cité Les Halles (Paris 1er) : programme et horaires | Cinemap</title>"
    ));
    assert!(html.contains("<h2>Séances du jeudi 1er janvier</h2>"));
    assert!(html.contains(
        "<a href=\"/film/1001-l-ete-dernier\">L&#39;Été dernier</a> : 14h00 (VF), 22h30 (VOST), 00h15 (VF)"
    ));
    assert!(html.contains("accepte Pathé CinéPass et UGC Illimité"));
    assert!(html.contains("\"@type\":\"MovieTheater\""));
    assert!(html.contains("\"startDate\":\"2099-01-01T14:00:00+01:00\""));
    assert!(html.contains("\"streetAddress\":\"7 Place de la Rotonde\""));
}

#[tokio::test]
async fn unknown_pages_are_real_404s() {
    let state = state().await;
    for uri in [
        "/film/9999",
        "/film/abc",
        "/cinema/OLD",
        "/cinema/NOGEO",
        "/nimporte/quoi",
    ] {
        let reply = get(&state, uri).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            reply
                .body
                .contains("<meta name=\"robots\" content=\"noindex\" />"),
            "{uri}"
        );
        assert!(!reply.body.contains("rel=\"canonical\""), "{uri}");
    }
    // Sous /api/, le contrat JSON reste le même.
    let reply = get(&state, "/api/nimporte").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(reply.body.contains("\"code\":\"not_found\""));
}

#[tokio::test]
async fn home_and_programme() {
    let state = state().await;
    let home = get(&state, "/").await;
    assert_eq!(home.status, StatusCode::OK);
    assert!(
        home.body
            .contains("<h1>Séances de cinéma près de chez vous</h1>")
    );
    assert!(home.body.contains("\"@type\":\"WebSite\""));
    let programme = get(&state, "/programme").await;
    assert_eq!(programme.status, StatusCode::OK);
    assert!(programme.body.contains("noindex"));
}

#[tokio::test]
async fn sitemap_lists_visible_cinemas_and_current_movies() {
    let state = state().await;
    let reply = get(&state, "/sitemap.xml").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.content_type.as_deref(),
        Some("application/xml; charset=utf-8")
    );
    let locs: Vec<&str> = reply
        .body
        .lines()
        .filter_map(|l| {
            l.strip_prefix("<url><loc>https://cinemap.ethansaux.fr")?
                .strip_suffix("</loc></url>")
        })
        .collect();
    assert_eq!(
        locs,
        [
            "/",
            "/film/1001-l-ete-dernier",
            "/film/1002-batman",
            "/cinema/LYON-pathe-bellecour-lyon",
            "/cinema/PARIS1-ugc-cine-cite-les-halles-paris",
            "/cinema/PARIS2-le-melies-montreuil",
        ]
    );
}

#[tokio::test]
async fn without_front_build_pages_are_disabled() {
    let state = AppState::new(state().await.pool);
    let reply = get(&state, "/film/1001-l-ete-dernier").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(reply.body.contains("\"code\":\"not_found\""));
}
