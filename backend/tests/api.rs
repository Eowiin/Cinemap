//! Tests du contrat (`docs/API.md`) : requêtes HTTP en mémoire, sans ouvrir de port,
//! sur la base de `tests/fixtures/api_seed.sql`.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use backend::api::{AppState, router};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

const PARIS: &str = "lat=48.8566&lng=2.3522";
const D1: &str = "2099-01-01";
const D2: &str = "2099-01-02";

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
    AppState::new(pool)
}

struct Reply {
    status: StatusCode,
    cache_control: Option<String>,
    json: Value,
}

async fn get(state: &AppState, uri: &str) -> Reply {
    let response = router(state.clone())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        cache_control,
        json,
    }
}

fn ids(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect()
}

fn showtime_ids(movies: &Value) -> Vec<&str> {
    movies
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["showtimes"].as_array().unwrap())
        .map(|s| s["id"].as_str().unwrap())
        .collect()
}

fn versions(movies: &Value) -> Vec<&str> {
    movies
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["showtimes"].as_array().unwrap())
        .map(|s| s["version"].as_str().unwrap())
        .collect()
}

fn assert_bad_request(reply: &Reply, uri: &str) {
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{uri}");
    assert_eq!(reply.json["error"]["code"], "bad_request", "{uri}");
    assert!(reply.json["error"]["message"].is_string(), "{uri}");
}

#[tokio::test]
async fn cinemas_lists_only_visible_cinemas_by_name() {
    let state = state().await;
    let reply = get(&state, "/api/cinemas").await;
    assert_eq!(reply.status, StatusCode::OK);
    // name_search : "le melies" < "pathe bellecour" < "ugc cine cite …"
    assert_eq!(ids(&reply.json), ["PARIS2", "LYON", "PARIS1"]);
    assert!(reply.json[0]["distance_km"].is_null());
}

#[tokio::test]
async fn cinemas_with_position_are_sorted_by_rounded_distance() {
    let state = state().await;
    let reply = get(&state, &format!("/api/cinemas?{PARIS}")).await;
    assert_eq!(ids(&reply.json), ["PARIS1", "PARIS2", "LYON"]);
    assert_eq!(reply.json[0]["distance_km"], 1.0);
    assert_eq!(reply.json[1]["distance_km"], 10.0);
}

#[tokio::test]
async fn cinemas_filter_art_et_essai() {
    let state = state().await;
    let reply = get(&state, "/api/cinemas?art_et_essai=true").await;
    assert_eq!(ids(&reply.json), ["PARIS1"]);
    assert_eq!(reply.json[0]["art_et_essai"], true);
}

#[tokio::test]
async fn hidden_cinemas_are_not_found() {
    let state = state().await;
    for id in ["NOGEO", "OLD", "XXXX"] {
        let reply = get(&state, &format!("/api/cinemas/{id}")).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{id}");
        assert_eq!(reply.json["error"]["code"], "not_found");
        let reply = get(&state, &format!("/api/cinemas/{id}/showtimes?date={D1}")).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{id}/showtimes");
    }
}

#[tokio::test]
async fn cinema_detail_has_every_field() {
    let state = state().await;
    let reply = get(&state, "/api/cinemas/PARIS1").await;
    assert_eq!(reply.status, StatusCode::OK);
    let cinema = &reply.json;
    assert_eq!(cinema["department"], "Paris");
    assert_eq!(
        cinema["allocine_url"],
        "https://www.allocine.fr/seance/salle_gen_csalle=PARIS1.html"
    );
    for field in [
        "id",
        "name",
        "city",
        "lat",
        "lng",
        "art_et_essai",
        "cards",
        "distance_km",
        "address",
        "postal_code",
        "department",
        "screens",
        "seats",
        "allocine_url",
    ] {
        assert!(cinema.get(field).is_some(), "champ absent : {field}");
    }
    assert!(cinema["screens"].is_null());
}

#[tokio::test]
async fn cinema_showtimes_group_by_movie_title() {
    let state = state().await;
    let reply = get(&state, &format!("/api/cinemas/PARIS1/showtimes?date={D1}")).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json["date"], D1);
    assert_eq!(reply.json["dates"], serde_json::json!([D1, D2]));
    let movies = &reply.json["movies"];
    // "batman" < "l'ete dernier"
    let titles: Vec<&str> = movies
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["movie"]["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Batman", "L'Été dernier"]);
    assert_eq!(showtime_ids(movies), ["B2", "A1", "A2", "A3"]);
    assert_eq!(
        movies[1]["showtimes"][0]["formats"],
        serde_json::json!(["IMAX"])
    );
    assert_eq!(movies[1]["movie"]["genres"], serde_json::json!(["Drame"]));
}

#[tokio::test]
async fn after_keeps_the_showtime_past_midnight() {
    let state = state().await;
    let reply = get(
        &state,
        &format!("/api/cinemas/PARIS1/showtimes?date={D1}&after=22:00"),
    )
    .await;
    assert_eq!(showtime_ids(&reply.json["movies"]), ["A2", "A3"]);

    let reply = get(&state, &format!("/api/cinemas/PARIS1/showtimes?date={D2}")).await;
    assert_eq!(showtime_ids(&reply.json["movies"]), ["A4"]);
}

#[tokio::test]
async fn version_filter() {
    let state = state().await;
    let reply = get(
        &state,
        &format!("/api/cinemas/PARIS1/showtimes?date={D1}&version=VO"),
    )
    .await;
    let mut vo = versions(&reply.json["movies"]);
    vo.sort_unstable();
    assert_eq!(vo, ["VO", "VOST"]);

    let reply = get(
        &state,
        &format!("/api/cinemas/PARIS1/showtimes?date={D1}&version=VF"),
    )
    .await;
    assert!(versions(&reply.json["movies"]).iter().all(|v| *v == "VF"));
    assert_eq!(showtime_ids(&reply.json["movies"]), ["A1", "A3"]);
}

#[tokio::test]
async fn date_without_showtimes_is_empty_not_404() {
    let state = state().await;
    let reply = get(&state, "/api/cinemas/PARIS1/showtimes?date=2098-01-01").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json["movies"], serde_json::json!([]));
}

#[tokio::test]
async fn now_showing_counts_visible_cinemas_only() {
    let state = state().await;
    let reply = get(&state, &format!("/api/movies?date={D1}")).await;
    assert_eq!(reply.status, StatusCode::OK);
    let movies = reply.json["movies"].as_array().unwrap();
    let summary: Vec<(i64, i64, i64)> = movies
        .iter()
        .map(|m| {
            (
                m["movie"]["id"].as_i64().unwrap(),
                m["cinema_count"].as_i64().unwrap(),
                m["showtime_count"].as_i64().unwrap(),
            )
        })
        .collect();
    // Film C (1003) absent, NOGEO et OLD non comptés.
    assert_eq!(summary, [(1001, 3, 5), (1002, 1, 1)]);
    assert_eq!(movies[0]["next_showtime"], "2099-01-01T14:00:00");
}

#[tokio::test]
async fn now_showing_with_position_radius_and_limit() {
    let state = state().await;
    let reply = get(
        &state,
        &format!("/api/movies?date={D1}&{PARIS}&radius_km=5"),
    )
    .await;
    let movies = reply.json["movies"].as_array().unwrap();
    assert_eq!(movies[0]["cinema_count"], 1);
    assert_eq!(movies.len(), 2);

    let reply = get(&state, &format!("/api/movies?date={D1}&limit=1")).await;
    assert_eq!(reply.json["movies"].as_array().unwrap().len(), 1);

    let reply = get(&state, &format!("/api/movies?date={D1}&after=18:30")).await;
    assert_eq!(
        reply.json["movies"][0]["next_showtime"],
        "2099-01-01T19:00:00"
    );

    // Aucun cinéma dans le rayon (au milieu de l'Atlantique).
    let reply = get(&state, &format!("/api/movies?date={D1}&lat=45&lng=-30")).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json["movies"], serde_json::json!([]));
}

#[tokio::test]
async fn movie_detail() {
    let state = state().await;
    let reply = get(&state, "/api/movies/1001").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json["title"], "L'Été dernier");
    assert_eq!(
        reply.json["cast"],
        serde_json::json!([{ "name": "Léa Drucker", "role": "Anne" }])
    );
    assert!(reply.json.get("trailer_url").is_some());

    // Sans séance, la fiche reste consultable.
    assert_eq!(get(&state, "/api/movies/1003").await.status, StatusCode::OK);
    let reply = get(&state, "/api/movies/999").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json["error"]["code"], "not_found");
}

#[tokio::test]
async fn movie_showtimes_by_distance() {
    let state = state().await;
    let reply = get(
        &state,
        &format!("/api/movies/1001/showtimes?date={D1}&{PARIS}"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json["movie"]["id"], 1001);
    assert_eq!(reply.json["dates"], serde_json::json!([D1, D2]));
    let cinemas = reply.json["cinemas"].as_array().unwrap();
    let names: Vec<(&str, f64, usize)> = cinemas
        .iter()
        .map(|c| {
            (
                c["cinema"]["id"].as_str().unwrap(),
                c["cinema"]["distance_km"].as_f64().unwrap(),
                c["showtimes"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(names, [("PARIS1", 1.0, 3), ("PARIS2", 10.0, 1)]);

    let reply = get(
        &state,
        &format!("/api/movies/1001/showtimes?date={D1}&{PARIS}&radius_km=5"),
    )
    .await;
    let cinemas = reply.json["cinemas"].as_array().unwrap();
    assert_eq!(cinemas.len(), 1);
    assert_eq!(cinemas[0]["cinema"]["id"], "PARIS1");
}

#[tokio::test]
async fn movie_showtimes_errors() {
    let state = state().await;
    let uri = "/api/movies/1001/showtimes";
    assert_bad_request(&get(&state, uri).await, uri);
    let uri = format!("/api/movies/999/showtimes?{PARIS}");
    assert_eq!(get(&state, &uri).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn search_ignores_case_and_accents() {
    let state = state().await;
    let reply = get(&state, "/api/search?q=cine%20cite").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(ids(&reply.json["cinemas"]), ["PARIS1"]);
    assert!(reply.json["cinemas"][0]["distance_km"].is_null());

    let reply = get(&state, "/api/search?q=montreuil").await;
    assert_eq!(ids(&reply.json["cinemas"]), ["PARIS2"]);
}

#[tokio::test]
async fn search_movies_are_upcoming_and_visible_only() {
    let state = state().await;
    let reply = get(&state, "/api/search?q=%C3%89T%C3%89").await; // "ÉTÉ"
    let movie_ids: Vec<i64> = reply.json["movies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(movie_ids, [1001]);

    // Film C sans séance, et "sans gps" n'est qu'un cinéma invisible.
    let reply = get(&state, "/api/search?q=sans").await;
    assert_eq!(reply.json["movies"], serde_json::json!([]));
    assert_eq!(reply.json["cinemas"], serde_json::json!([]));

    // "_" n'est pas un joker.
    let reply = get(&state, "/api/search?q=e_").await;
    assert_eq!(reply.json["cinemas"], serde_json::json!([]));
}

#[tokio::test]
async fn invalid_parameters_are_json_bad_requests() {
    let state = state().await;
    for uri in [
        "/api/cinemas/PARIS1/showtimes?date=2026-13-01",
        "/api/cinemas/PARIS1/showtimes?after=25:00",
        "/api/cinemas/PARIS1/showtimes?version=VOST",
        "/api/cinemas/PARIS1/showtimes?version=vf",
        "/api/cinemas?art_et_essai=oui",
        "/api/cinemas?lat=48",
        "/api/movies?limit=0",
        "/api/movies?limit=500",
        "/api/movies/abc",
        "/api/movies/abc/showtimes?lat=48&lng=2",
        "/api/movies/1001/showtimes?lat=48&lng=2&radius_km=101",
        "/api/search?q=a",
        "/api/search?q=%25",
        "/api/search",
    ] {
        assert_bad_request(&get(&state, uri).await, uri);
    }
}

#[tokio::test]
async fn cache_control_depends_on_status() {
    let state = state().await;
    let ok = get(&state, "/api/cinemas").await;
    assert_eq!(ok.cache_control.as_deref(), Some("public, max-age=300"));
    let not_found = get(&state, "/api/cinemas/XXXX").await;
    assert_eq!(not_found.cache_control.as_deref(), Some("no-store"));
    let unknown_route = get(&state, "/api/nimporte").await;
    assert_eq!(unknown_route.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_route.json["error"]["code"], "not_found");
    assert_eq!(unknown_route.cache_control.as_deref(), Some("no-store"));
}

#[tokio::test]
async fn meta_describes_visible_data() {
    let state = state().await;
    let reply = get(&state, "/api/meta").await;
    assert_eq!(reply.status, StatusCode::OK);
    let meta = &reply.json;
    // today dépend de l'horloge : on vérifie la forme, pas la valeur.
    let today = meta["today"].as_str().unwrap();
    assert_eq!(today.len(), 10);
    assert_eq!(meta["dates_available"], serde_json::json!([D1, D2]));
    assert!(meta["last_scrape_at"].is_null());
    assert_eq!(meta["cinema_count"], 3);
    assert_eq!(meta["movie_count"], 2);
}

#[tokio::test]
async fn cinemas_carry_their_cards_and_filter_on_them() {
    let state = state().await;
    let reply = get(&state, "/api/cinemas").await;
    let cards: Vec<(&str, &Value)> = reply
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), &c["cards"]))
        .collect();
    assert_eq!(
        cards,
        [
            ("PARIS2", &serde_json::json!([])),
            ("LYON", &serde_json::json!(["pathe_cinepass"])),
            (
                "PARIS1",
                &serde_json::json!(["pathe_cinepass", "ugc_illimite"])
            ),
        ]
    );

    let reply = get(&state, "/api/cinemas?cards=ugc_illimite").await;
    assert_eq!(ids(&reply.json), ["PARIS1"]);
    // « Au moins une » des cartes.
    let reply = get(&state, "/api/cinemas?cards=ugc_illimite,pathe_cinepass").await;
    assert_eq!(ids(&reply.json), ["LYON", "PARIS1"]);
    let reply = get(
        &state,
        "/api/cinemas?cards=pathe_cinepass&art_et_essai=false",
    )
    .await;
    assert_eq!(ids(&reply.json), ["LYON"]);
    let reply = get(
        &state,
        &format!("/api/cinemas?cards=pathe_cinepass&{PARIS}"),
    )
    .await;
    assert_eq!(ids(&reply.json), ["PARIS1", "LYON"]);

    let reply = get(&state, "/api/cinemas/PARIS1").await;
    assert_eq!(
        reply.json["cards"],
        serde_json::json!(["pathe_cinepass", "ugc_illimite"])
    );
}

#[tokio::test]
async fn movies_filter_on_cards() {
    let state = state().await;
    let reply = get(
        &state,
        &format!("/api/movies?date={D1}&cards=pathe_cinepass"),
    )
    .await;
    let counts: Vec<(i64, i64)> = reply.json["movies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["movie"]["id"].as_i64().unwrap(),
                m["cinema_count"].as_i64().unwrap(),
            )
        })
        .collect();
    // PARIS1 et LYON acceptent la carte, PARIS2 non.
    assert_eq!(counts, [(1001, 2), (1002, 1)]);

    let reply = get(
        &state,
        &format!("/api/movies?date={D1}&{PARIS}&radius_km=15&cards=ugc_illimite"),
    )
    .await;
    assert_eq!(reply.json["movies"][0]["cinema_count"], 1);

    let reply = get(
        &state,
        &format!("/api/movies/1001/showtimes?date={D1}&{PARIS}&radius_km=15&cards=ugc_illimite"),
    )
    .await;
    let cinemas: Vec<&str> = reply.json["cinemas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["cinema"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(cinemas, ["PARIS1"]);
}

#[tokio::test]
async fn unknown_or_empty_cards_are_bad_requests() {
    let state = state().await;
    for uri in [
        "/api/cinemas?cards=carte_inconnue",
        "/api/cinemas?cards=",
        "/api/movies?cards=ugc_illimite,carte_inconnue",
    ] {
        let reply = get(&state, uri).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(reply.json["error"]["code"], "bad_request", "{uri}");
    }
}

#[tokio::test]
async fn meta_lists_cards_by_name() {
    let state = state().await;
    let reply = get(&state, "/api/meta").await;
    assert_eq!(
        reply.json["cards"],
        serde_json::json!([
            { "id": "pathe_cinepass", "name": "Pathé CinéPass", "updated_at": "2099-01-01T02:14:00Z" },
            { "id": "ugc_illimite", "name": "UGC Illimité", "updated_at": "2099-01-01T02:14:00Z" },
        ])
    );
}
