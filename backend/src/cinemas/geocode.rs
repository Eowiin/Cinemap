use std::collections::HashMap;

use csv::Reader;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use sqlx::{SqlitePool, query};

use tracing::{info, warn};

use super::allocine::Cinema;
use crate::text::{department_from_insee, normalize};

#[derive(Debug, Deserialize)]
struct GeoData {
    id: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    result_score: Option<f64>,
    result_postcode: String,
    result_city: String,
    result_citycode: String,
    result_status: String,
}

pub(super) async fn geocode_cinemas(
    pool: &SqlitePool,
    client: &reqwest::Client,
    cinemas: &HashMap<String, Cinema>,
) -> anyhow::Result<()> {
    let Some(csv_data) = build_cinemas_csv(pool).await? else {
        info!("Aucun géocodage nécessaire");
        return Ok(());
    };

    let raw_csv = client
        .post("https://data.geopf.fr/geocodage/search/csv")
        .multipart(csv_data)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let results = get_geodata(&raw_csv)?;
    save_geodata(pool, &results, cinemas).await?;
    Ok(())
}

async fn save_geodata(
    pool: &SqlitePool,
    results: &[GeoData],
    cinemas: &HashMap<String, Cinema>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    for data in results {
        let cinema = cinemas.get(&data.id);
        let city_search = normalize(&data.result_city);
        let department = department_from_insee(&data.result_citycode);

        if data.result_status != "ok" || data.result_score.is_none_or(|score| score < 0.5) {
            warn!(
                id = %data.id,
                name = ?cinema.map(|c| c.name.as_str()),
                address = ?cinema.and_then(|c| c.address.as_deref()),
                status = %data.result_status,
                score = ?data.result_score,
                "Géocodage absent ou peu fiable"
            );
        }

        if data.result_status != "ok" || data.latitude.is_none() || data.longitude.is_none() {
            continue;
        }

        sqlx::query!(
            r#"
            UPDATE cinemas
            SET
                lat = ?1,
                lng = ?2,
                geocode_score = ?3,
                postal_code = ?4,
                city = ?5,
                city_search = ?6,
                insee_code = ?7,
                department = ?8
            WHERE id = ?9
            "#,
            data.latitude,
            data.longitude,
            data.result_score,
            data.result_postcode,
            data.result_city,
            city_search,
            data.result_citycode,
            &department,
            data.id
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn build_cinemas_csv(pool: &SqlitePool) -> anyhow::Result<Option<Form>> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    let data = query!(
        r#"
        SELECT id AS "id!", address AS "address!"
        FROM cinemas
        WHERE id IS NOT NULL
            AND address IS NOT NULL
            AND trim(address) <> ''
            AND (lat IS NULL OR lng IS NULL)
        "#
    )
    .fetch_all(pool)
    .await?;

    if data.is_empty() {
        return Ok(None);
    }

    wtr.write_record(["id", "adresse"])?;
    for cinema in data {
        wtr.write_record([cinema.id, cinema.address])?;
    }
    let csv_data = wtr.into_inner()?;
    let part = Part::bytes(csv_data)
        .file_name("cinemas.csv")
        .mime_str("text/csv")?;
    let form = Form::new().part("data", part).text("columns", "adresse");

    Ok(Some(form))
}

fn get_geodata(raw_csv: &str) -> anyhow::Result<Vec<GeoData>> {
    let mut reader = Reader::from_reader(raw_csv.as_bytes());
    let geodata = reader
        .deserialize::<GeoData>()
        .collect::<Result<Vec<_>, csv::Error>>()?;

    Ok(geodata)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture synthétique : colonnes d'entrée conservées, longitude avant
    // latitude et colonne supplémentaire pour vérifier le mapping par en-tête.
    const RESPONSE: &str = include_str!("../../tests/fixtures/geocodage.csv");

    #[test]
    fn parses_geocoding_by_column_name() {
        let results = get_geodata(RESPONSE).expect("Réponse CSV valide");
        assert_eq!(results.len(), 2);

        let cinema = &results[0];
        assert_eq!(cinema.id, "C0159");
        assert!((cinema.latitude.unwrap() - 48.862712345).abs() < 1e-10);
        assert!((cinema.longitude.unwrap() - 2.346912345).abs() < 1e-10);
        assert_eq!(cinema.result_score, Some(0.91));
        assert_eq!(cinema.result_postcode, "75001");
        assert_eq!(cinema.result_city, "Paris");
        assert_eq!(cinema.result_citycode, "75101");
        assert_eq!(cinema.result_status, "ok");
    }

    #[test]
    fn skipped_row_does_not_abort_the_batch() {
        let results = get_geodata(RESPONSE).expect("Une ligne skipped reste valide");
        assert_eq!(results.len(), 2);

        let skipped = &results[1];
        assert_eq!(skipped.id, "UNKNOWN");
        assert_eq!(skipped.result_status, "skipped");
        assert_eq!(skipped.latitude, None);
        assert_eq!(skipped.longitude, None);
        assert_eq!(skipped.result_score, None);
        assert!(skipped.result_postcode.is_empty());
        assert!(skipped.result_city.is_empty());
        assert!(skipped.result_citycode.is_empty());
    }

    #[test]
    fn invalid_numeric_value_is_reported() {
        let invalid_response = RESPONSE.replace("48.862712345", "invalid");
        assert!(get_geodata(&invalid_response).is_err());
    }
}
