pub mod allocine;
pub mod departments;
mod geocode;

use std::collections::HashMap;

use allocine::get_cinemas_from_department;
use departments::get_departments;
use sqlx::SqlitePool;
use tracing::info;

use crate::{cinemas::allocine::Cinema, client::build_client, text::normalize};

pub async fn import_cinemas(pool: &SqlitePool) -> anyhow::Result<()> {
    let departments = get_departments();
    let client = build_client()?;
    let mut cinemas_map: HashMap<String, Cinema> = HashMap::new();
    let mut duplicates = 0;

    for department in departments {
        let cinemas = get_cinemas_from_department(&department, &client).await?;

        for cinema in cinemas {
            if cinemas_map.insert(cinema.id.clone(), cinema).is_some() {
                duplicates += 1;
            }
        }
    }
    upsert_cinemas(pool, &cinemas_map).await?;
    geocode::geocode_cinemas(pool, &client, &cinemas_map).await?;

    info!(
        "{} cinémas, {} doublons ignorés",
        cinemas_map.len(),
        duplicates
    );
    Ok(())
}

async fn upsert_cinemas(
    pool: &SqlitePool,
    cinemas: &HashMap<String, Cinema>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    for cinema in cinemas.values() {
        let name_search = normalize(&cinema.name);

        sqlx::query!(
            r#"
            INSERT INTO cinemas (
                id, name, name_search, address, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, datetime('now'))

            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                name_search = excluded.name_search,

                lat = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.lat
                END,

                lng = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.lng
                END,

                geocode_score = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.geocode_score
                END,

                insee_code = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.insee_code
                END,

                address = excluded.address,
                updated_at = excluded.updated_at
            "#,
            cinema.id,
            cinema.name,
            name_search,
            cinema.address
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
