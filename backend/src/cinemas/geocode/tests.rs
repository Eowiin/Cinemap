use super::*;
use axum::{Router, body::Bytes, extract::State, http::StatusCode, routing::post};
use std::sync::{Arc, Mutex};

const RESPONSE: &str = include_str!("../../../tests/fixtures/geocodage.csv");

fn sample() -> GeoData {
    get_geodata(RESPONSE).unwrap().remove(0)
}

#[test]
fn extracts_only_complete_postal_codes() {
    for (address, expected) in [
        ("75015 Paris 15e arrondissement", Some("75015")),
        ("BP 30317 56000 Vannes", Some("56000")),
        ("École 75001 Paris", Some("75001")),
        ("123456 Paris", None),
        ("Paris 1234567", None),
        ("Rue du cinéma", None),
        ("", None),
        ("75001", Some("75001")),
        ("31 300 Toulouse", None),
    ] {
        assert_eq!(
            get_postal_code(address).map(|i| &address[i..i + 5]),
            expected,
            "{address}"
        );
    }
}

#[test]
fn prepares_multiline_and_single_line_addresses() {
    for (input, expected, postcode, city) in [
        (
            "128-162 Av. de France\naccès Bibliothèque 75013 Paris",
            "128-162 Av. de France 75013 Paris",
            "75013",
            "Paris",
        ),
        (
            "35 Rue Gambetta\nSalle Polyvalente 10110 Bar-sur-Seine",
            "35 Rue Gambetta 10110 Bar-sur-Seine",
            "10110",
            "Bar-sur-Seine",
        ),
        (
            "36 Rue de Montceau-les-Mines 62440 Harnes",
            "36 Rue de Montceau-les-Mines 62440 Harnes",
            "62440",
            "Harnes",
        ),
        (
            "Rue Louis Husson\nSalle polyvalente 10220 Piney",
            "Rue Louis Husson 10220 Piney",
            "10220",
            "Piney",
        ),
        (
            "  Rue A 31 300 Toulouse  ",
            "Rue A 31300 Toulouse",
            "31300",
            "Toulouse",
        ),
        (
            "BP 30317 56000 Vannes",
            "BP 30317 56000 Vannes",
            "56000",
            "Vannes",
        ),
    ] {
        let address = prepare_address(input);
        assert_eq!(address.full_query, expected);
        assert_eq!(address.postcode.as_deref(), Some(postcode));
        assert_eq!(address.city.as_deref(), Some(city));
    }
    assert_eq!(
        normalize_postal_spacing("12 345 Rue A 75001 Paris"),
        "12 345 Rue A 75001 Paris"
    );
    assert_eq!(normalize_postal_spacing("12 345 Rue A"), "12 345 Rue A");
    let missing = prepare_address("Rue du cinéma\nSalle des fêtes");
    assert_eq!(missing.full_query, "Rue du cinéma Salle des fêtes");
    assert!(missing.postcode.is_none());
    assert!(missing.municipality_query.is_none());
}

#[test]
fn postal_filter_and_coordinates_matter_more_than_score() {
    let address = prepare_address("7 Place de la Rotonde 75001 Paris");
    let mut result = sample();
    result.result_score = Some(0.1);
    assert!(validate_result(&address, &result, false).is_ok());
    result.result_score = Some(0.99);
    result.result_postcode = "13001".into();
    assert!(validate_result(&address, &result, false).is_err());
    result.result_postcode = "75001".into();
    result.result_citycode = "75056".into();
    assert!(validate_result(&address, &result, false).is_err());
    result.result_citycode = "75101".into();
    result.latitude = Some(f64::NAN);
    assert!(validate_result(&address, &result, false).is_err());
    result.latitude = Some(91.0);
    assert!(validate_result(&address, &result, false).is_err());
    assert!(validate_result(&prepare_address("Paris"), &sample(), false).is_err());
}

#[test]
fn fallback_accepts_different_postcode_but_not_different_city() {
    let address = prepare_address("Rue A 31300 Toulouse");
    let mut result = get_geodata(RESPONSE).unwrap().remove(2);
    assert!(validate_result(&address, &result, true).is_ok());
    assert!(validate_result(&address, &result, false).is_err());
    result.result_city = "Colomiers".into();
    assert!(validate_result(&address, &result, true).is_err());
    result.result_city = "Toulouse".into();
    result.result_citycode = "11001".into();
    assert!(validate_result(&address, &result, true).is_err());
    assert_eq!(
        city_key("Évry-Courcouronnes"),
        city_key("Evry Courcouronnes")
    );
}

#[test]
fn department_and_arrondissement_rules() {
    for (postcode, code, valid) in [
        ("20000", "2A004", true),
        ("20200", "2B033", true),
        ("20000", "13001", false),
        ("97100", "97105", true),
        ("97100", "97201", false),
        ("97600", "97611", true),
        ("75013", "75113", true),
        ("75013", "13201", false),
    ] {
        assert_eq!(department_matches(postcode, code), valid);
    }
    for (address, code, query) in [
        ("75013 Paris", "75113", "Paris 13e Arrondissement"),
        ("69001 Lyon", "69381", "Lyon 1er Arrondissement"),
        ("69009 Lyon", "69389", "Lyon 9e Arrondissement"),
        ("13008 Marseille", "13208", "Marseille 8e Arrondissement"),
        ("13016 Marseille", "13216", "Marseille 16e Arrondissement"),
    ] {
        let address = prepare_address(address);
        assert_eq!(address.arrondissement.as_deref(), Some(code));
        assert_eq!(address.municipality_query.as_deref(), Some(query));
        let mut result = sample();
        result.result_type = "municipality".into();
        result.result_citycode = code.into();
        assert!(validate_result(&address, &result, true).is_ok());
        result.result_citycode = "75056".into();
        assert!(validate_result(&address, &result, true).is_err());
    }
}

#[test]
fn parses_csv_by_header_and_preserves_skipped_rows() {
    let results = get_geodata(RESPONSE).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "C0159");
    assert!((results[0].latitude.unwrap() - 48.862712345).abs() < 1e-10);
    assert!((results[0].longitude.unwrap() - 2.346912345).abs() < 1e-10);
    assert_eq!(results[0].result_type, "housenumber");
    assert_eq!(results[1].result_status, "skipped");
    assert!(results[1].latitude.is_none());
    assert!(get_geodata(&RESPONSE.replace("48.862712345", "invalid")).is_err());
}

async fn database() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    sqlx::query("INSERT INTO cinemas (id, name, name_search, address, lat, lng, postal_code, city, city_search, insee_code, department, geocode_score, geocode_type, updated_at) VALUES ('TOULOUSE', 'Cinéma', 'cinema', 'Rue A 31300 Toulouse', 1, 2, '13001', 'Marseille', 'marseille', '13201', '13', 0.9, 'street', 'now')")
        .execute(&pool).await.unwrap();
    pool
}

type Replies = Arc<Mutex<Vec<(StatusCode, String)>>>;
type Bodies = Arc<Mutex<Vec<String>>>;

struct Mock {
    url: String,
    bodies: Bodies,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn mock(replies: Vec<(StatusCode, String)>) -> Mock {
    async fn respond(
        State((replies, bodies)): State<(Replies, Bodies)>,
        body: Bytes,
    ) -> (StatusCode, String) {
        bodies
            .lock()
            .unwrap()
            .push(String::from_utf8(body.to_vec()).unwrap());
        let mut replies = replies.lock().unwrap();
        if replies.is_empty() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected request".into(),
            );
        }
        replies.remove(0)
    }
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let replies = Arc::new(Mutex::new(replies));
    let app = Router::new()
        .route("/", post(respond))
        .with_state((replies, bodies.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Mock { url, bodies, task }
}

fn toulouse_response() -> String {
    let lines: Vec<_> = RESPONSE.lines().collect();
    format!("{}\n{}\n", lines[0], lines[3])
}

fn fallback_response(response: &str) -> String {
    let mut reader = csv::Reader::from_reader(response.as_bytes());
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(reader.headers().unwrap()).unwrap();
    for row in reader.records() {
        let row = row.unwrap();
        for suffix in ["address", "municipality"] {
            let mut fields: Vec<_> = row.iter().map(str::to_owned).collect();
            fields[0] = format!("{}:{suffix}", fields[0]);
            writer.write_record(fields).unwrap();
        }
    }
    String::from_utf8(writer.into_inner().unwrap()).unwrap()
}

#[tokio::test]
async fn two_passes_save_fallback_and_source_postcode() {
    let pool = database().await;
    assert!(
        load_cinemas_to_geocode(&pool, false)
            .await
            .unwrap()
            .is_empty()
    );
    let server = mock(vec![
        (StatusCode::OK, toulouse_response()),
        (StatusCode::OK, fallback_response(&toulouse_response())),
    ])
    .await;
    geocode_cinemas_at(&pool, &reqwest::Client::new(), true, &server.url)
        .await
        .unwrap();
    let row: (String, String, String, String) =
        sqlx::query_as("SELECT postal_code, city, department, geocode_type FROM cinemas")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        (
            "31300".into(),
            "Toulouse".into(),
            "31".into(),
            "municipality".into()
        )
    );
    let bodies = server.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].contains("id,adresse,postcode"));
    assert!(bodies[0].contains("TOULOUSE,Rue A 31300 Toulouse,31300"));
    assert!(bodies[0].contains("name=\"postcode\""));
    assert!(bodies[1].contains("TOULOUSE:address,Rue A 31300 Toulouse,,"));
    assert!(bodies[1].contains("TOULOUSE:municipality,31300 Toulouse,,municipality"));
    assert!(bodies[1].contains("name=\"type\""));
}

#[tokio::test]
async fn rejected_result_clears_previous_geodata() {
    let pool = database().await;
    let wrong_city = toulouse_response().replace(",Toulouse,31555", ",Colomiers,31149");
    let server = mock(vec![
        (StatusCode::OK, wrong_city.clone()),
        (StatusCode::OK, fallback_response(&wrong_city)),
    ])
    .await;
    geocode_cinemas_at(&pool, &reqwest::Client::new(), true, &server.url)
        .await
        .unwrap();
    let cleared: bool = sqlx::query_scalar("SELECT lat IS NULL AND lng IS NULL AND city IS NULL AND city_search IS NULL AND insee_code IS NULL AND department IS NULL AND geocode_type IS NULL AND geocode_score IS NULL AND postal_code = '31300' FROM cinemas")
        .fetch_one(&pool).await.unwrap();
    assert!(cleared);
}

#[tokio::test]
async fn http_or_csv_errors_preserve_old_positions() {
    let header = RESPONSE.lines().next().unwrap().to_owned();
    for reply in [
        (StatusCode::SERVICE_UNAVAILABLE, "down".into()),
        (StatusCode::OK, header),
        (
            StatusCode::OK,
            toulouse_response().replace("TOULOUSE", "UNEXPECTED"),
        ),
        (
            StatusCode::OK,
            toulouse_response().replace("43.604082", "invalid"),
        ),
    ] {
        let pool = database().await;
        let server = mock(vec![(StatusCode::OK, toulouse_response()), reply]).await;
        assert!(
            geocode_cinemas_at(&pool, &reqwest::Client::new(), true, &server.url)
                .await
                .is_err()
        );
        let row: (f64, String) = sqlx::query_as("SELECT lat, city FROM cinemas")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row, (1.0, "Marseille".into()));
    }
}

#[tokio::test]
async fn missing_address_is_invalidated_without_http() {
    let pool = database().await;
    sqlx::query("UPDATE cinemas SET address = NULL")
        .execute(&pool)
        .await
        .unwrap();
    geocode_cinemas_at(&pool, &reqwest::Client::new(), true, "http://127.0.0.1:1")
        .await
        .unwrap();
    let latitude: Option<f64> = sqlx::query_scalar("SELECT lat FROM cinemas")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(latitude.is_none());
}

#[tokio::test]
async fn concurrent_address_change_prevents_stale_write() {
    let pool = database().await;
    let cinemas = load_cinemas_to_geocode(&pool, true).await.unwrap();
    let prepared = vec![prepare_address(cinemas[0].address.as_deref().unwrap())];
    sqlx::query("UPDATE cinemas SET address = 'Nouvelle adresse'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        save_geodata(&pool, &cinemas, &prepared, &[None])
            .await
            .is_err()
    );
    let latitude: f64 = sqlx::query_scalar("SELECT lat FROM cinemas")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(latitude, 1.0);
}

#[tokio::test]
async fn precise_result_needs_only_one_request() {
    let pool = database().await;
    let response = toulouse_response()
        .replace(",31000,", ",31300,")
        .replace("municipality", "street");
    let server = mock(vec![(StatusCode::OK, response)]).await;
    geocode_cinemas_at(&pool, &reqwest::Client::new(), true, &server.url)
        .await
        .unwrap();
    assert_eq!(server.bodies.lock().unwrap().len(), 1);
    assert!(
        load_cinemas_to_geocode(&pool, false)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn duplicate_response_rows_are_protocol_errors() {
    let pool = database().await;
    let mut response = toulouse_response();
    response.push_str(toulouse_response().lines().nth(1).unwrap());
    let server = mock(vec![(StatusCode::OK, response)]).await;
    assert!(
        geocode_cinemas_at(&pool, &reqwest::Client::new(), true, &server.url)
            .await
            .is_err()
    );
    let latitude: f64 = sqlx::query_scalar("SELECT lat FROM cinemas")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(latitude, 1.0);
}

#[tokio::test]
async fn sql_failure_rolls_back_the_whole_batch() {
    let pool = database().await;
    sqlx::query("INSERT INTO cinemas (id, name, name_search, address, lat, lng, updated_at) VALUES ('Z', 'Z', 'z', '31300 Toulouse', 3, 4, 'now')")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_geocoding BEFORE UPDATE ON cinemas WHEN NEW.id = 'Z' BEGIN SELECT RAISE(ABORT, 'simulated failure'); END")
        .execute(&pool).await.unwrap();
    let cinemas = load_cinemas_to_geocode(&pool, true).await.unwrap();
    let prepared: Vec<_> = cinemas
        .iter()
        .map(|c| prepare_address(c.address.as_deref().unwrap()))
        .collect();
    assert!(
        save_geodata(&pool, &cinemas, &prepared, &[None, None])
            .await
            .is_err()
    );
    let latitudes: Vec<f64> = sqlx::query_scalar("SELECT lat FROM cinemas ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(latitudes, vec![1.0, 3.0]);
}

#[test]
fn multiline_postal_code_takes_precedence_over_po_box() {
    let address = prepare_address("Rue A BP 30317\n56000 Vannes");
    assert_eq!(address.postcode.as_deref(), Some("56000"));
    assert_eq!(address.full_query, "Rue A BP 30317 56000 Vannes");
}

#[test]
fn real_localities_and_cedex_responses_are_validated_by_place() {
    // IGN responses captured on 2026-10-05, including deliberately incorrect matches.
    let raw = include_str!("../../../tests/fixtures/geocodage_localities.csv");
    let mut source = csv::Reader::from_reader(raw.as_bytes());
    let addresses: HashMap<String, String> = source
        .records()
        .map(|r| {
            let r = r.unwrap();
            (r[0].to_owned(), r[1].to_owned())
        })
        .collect();
    for data in get_geodata(raw).unwrap() {
        let address = prepare_address(&addresses[&data.id]);
        let accepted = matches!(
            data.id.as_str(),
            "B0144"
                | "G02M3"
                | "P0665"
                | "P0810"
                | "P1033"
                | "P1092"
                | "W0100"
                | "W1239"
                | "W3057"
                | "W4631"
                | "W4822"
                | "W4824"
                | "W4918"
        );
        assert_eq!(
            validate_result(&address, &data, true).is_ok(),
            accepted,
            "{}",
            data.id
        );
        if matches!(data.id.as_str(), "P0665" | "P0810" | "W4918") {
            assert!(
                validate_result(&address, &data, false).is_err(),
                "CEDEX needs a retry"
            );
        }
    }
}

#[test]
fn same_department_or_road_name_is_not_enough() {
    let address = prepare_address("68070 Mulhouse");
    let mut data = sample();
    data.result_citycode = "68056".into();
    data.result_city = "Brunstatt-Didenheim".into();
    data.result_context = "68, Haut-Rhin, Grand Est".into();
    data.result_name = "Rue de Mulhouse".into();
    for kind in ["street", "locality", "municipality"] {
        data.result_type = kind.into();
        assert!(validate_result(&address, &data, true).is_err());
    }
    data.result_city = "Mulhouse".into();
    assert!(validate_result(&address, &data, true).is_ok());
    data.result_context = "67, Bas-Rhin, Grand Est".into();
    assert!(validate_result(&address, &data, true).is_err());
}

#[test]
fn shortened_names_require_same_postcode_and_prefix_boundary() {
    let address = prepare_address("73210 La Plagne");
    let mut data = sample();
    data.result_type = "locality".into();
    data.result_citycode = "73150".into();
    data.result_postcode = "73210".into();
    data.result_city = "La Plagne Tarentaise".into();
    assert!(validate_result(&address, &data, true).is_ok());
    data.result_postcode = "73400".into();
    assert!(validate_result(&address, &data, true).is_err());
    data.result_postcode = "73210".into();
    data.result_city = "Aime-la-Plagne".into();
    assert!(validate_result(&address, &data, true).is_err());
    data.result_city = "La Plagnette".into();
    assert!(validate_result(&address, &data, true).is_err());
}
