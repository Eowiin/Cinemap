pub mod allocine;
pub mod cnc;
pub mod departments;
pub mod geocode;

use std::collections::HashSet;

use allocine::get_cinemas_from_department;
use anyhow::Context;
use departments::get_departments;
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::{cinemas::allocine::Cinema, client::build_client, text::normalize};

pub async fn import_cinemas(pool: &SqlitePool) -> anyhow::Result<()> {
    let departments = get_departments();
    let client = build_client()?;
    // Même format que `updated_at` : sert au rapport pour repérer les cinémas absents de cet import.
    let started_at = sqlx::query_scalar!(r#"SELECT datetime('now') AS "now!: String""#)
        .fetch_one(pool)
        .await?;
    // Seuls les IDs servent à compter les doublons : inutile de garder les cinémas entiers.
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut duplicates = 0;

    for department in departments {
        let cinemas = get_cinemas_from_department(&department, &client)
            .await
            .with_context(|| {
                format!(
                    "Import du département {} ({})",
                    department.code_insee, department.nom
                )
            })?;

        upsert_cinemas(pool, &cinemas).await?;
        info!(
            department = %department.code_insee,
            name = %department.nom,
            cinemas = cinemas.len(),
            "Département enregistré"
        );

        for cinema in cinemas {
            if !seen_ids.insert(cinema.id) {
                duplicates += 1;
            }
        }
    }
    geocode::geocode_cinemas(pool, &client, false).await?;
    // Les cinémas sont déjà enregistrés : un CNC indisponible ne doit pas faire échouer
    // l'import. Les données CNC précédentes restent en place (remise à zéro seulement
    // après un téléchargement réussi).
    if let Err(error) = cnc::enrich_cinemas(pool, &client).await {
        warn!(
            error = format!("{error:#}"),
            "Enrichissement CNC impossible, ignoré"
        );
    }

    log_import_report(pool, &started_at, duplicates).await
}

/// Chiffres de fin d'import, tirés de la base.
#[derive(Debug, PartialEq)]
struct ImportReport {
    total: i64,
    without_position: i64,
    approximate: i64,
    low_score: i64,
    with_cnc: i64,
    /// Cinémas en base mais plus listés par AlloCiné depuis `started_at` (fermés ?).
    not_seen: i64,
}

/// Une seule requête : `count(col)` ne compte que les valeurs non `NULL`, et
/// `sum(condition)` compte les lignes où la condition vaut 1.
async fn import_report(pool: &SqlitePool, started_at: &str) -> anyhow::Result<ImportReport> {
    Ok(sqlx::query_as!(
        ImportReport,
        r#"SELECT
            count(*) AS "total!: i64",
            count(*) - count(lat) AS "without_position!: i64",
            coalesce(sum(geocode_type = 'municipality'), 0) AS "approximate!: i64",
            coalesce(sum(geocode_score < 0.5), 0) AS "low_score!: i64",
            count(cnc_id) AS "with_cnc!: i64",
            coalesce(sum(updated_at < ?), 0) AS "not_seen!: i64"
           FROM cinemas"#,
        started_at
    )
    .fetch_one(pool)
    .await?)
}

async fn log_import_report(
    pool: &SqlitePool,
    started_at: &str,
    duplicates: usize,
) -> anyhow::Result<()> {
    let report = import_report(pool, started_at).await?;
    info!(
        cinemas = report.total,
        sans_position = report.without_position,
        position_commune = report.approximate,
        score_faible = report.low_score,
        croises_cnc = report.with_cnc,
        non_croises_cnc = report.total - report.with_cnc,
        absents_de_cet_import = report.not_seen,
        doublons_ignores = duplicates,
        "Import des cinémas terminé"
    );
    Ok(())
}

async fn upsert_cinemas(pool: &SqlitePool, cinemas: &[Cinema]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    for cinema in cinemas {
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

                geocode_type = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.geocode_type
                END,

                department = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.department
                END,

                postal_code = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.postal_code
                END,

                city = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.city
                END,

                city_search = CASE
                    WHEN cinemas.address IS NOT excluded.address
                    THEN NULL ELSE cinemas.city_search
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn import_report_counts_each_category() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at, lat, lng, geocode_type, geocode_score, cnc_id) VALUES
             ('A', 'a', 'a', '2026-10-06 10:00:00', 48.8, 2.3, 'housenumber', 0.9, 1),
             ('B', 'b', 'b', '2026-10-06 10:00:00', 48.8, 2.3, 'municipality', 0.4, NULL),
             ('C', 'c', 'c', '2026-10-06 10:00:00', NULL, NULL, NULL, NULL, NULL),
             ('OLD', 'old', 'old', '2026-09-01 10:00:00', 48.8, 2.3, 'street', 0.8, 2)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let report = import_report(&pool, "2026-10-06 09:00:00").await.unwrap();

        assert_eq!(
            report,
            ImportReport {
                total: 4,
                without_position: 1,
                approximate: 1,
                low_score: 1,
                with_cnc: 2,
                not_seen: 1,
            }
        );
    }

    #[tokio::test]
    async fn changed_address_invalidates_all_geographic_fields() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let mut cinema = Cinema {
            id: "C0159".into(),
            name: "Cinéma".into(),
            address: Some("7 Place de la Rotonde 75001 Paris".into()),
        };
        upsert_cinemas(&pool, std::slice::from_ref(&cinema))
            .await
            .unwrap();
        sqlx::query("UPDATE cinemas SET lat = 48.86, lng = 2.34, geocode_score = 0.9, geocode_type = 'housenumber', insee_code = '75101', department = '75', postal_code = '75001', city = 'Paris', city_search = 'paris'")
            .execute(&pool).await.unwrap();
        upsert_cinemas(&pool, std::slice::from_ref(&cinema))
            .await
            .unwrap();
        let preserved: bool = sqlx::query_scalar("SELECT lat = 48.86 AND geocode_type = 'housenumber' AND postal_code = '75001' AND city = 'Paris' FROM cinemas")
            .fetch_one(&pool).await.unwrap();
        assert!(preserved);
        cinema.address = Some("Rue A 31300 Toulouse".into());
        upsert_cinemas(&pool, &[cinema]).await.unwrap();
        let cleared: bool = sqlx::query_scalar("SELECT lat IS NULL AND lng IS NULL AND geocode_score IS NULL AND geocode_type IS NULL AND insee_code IS NULL AND department IS NULL AND postal_code IS NULL AND city IS NULL AND city_search IS NULL FROM cinemas")
            .fetch_one(&pool).await.unwrap();
        assert!(cleared);
    }

    #[tokio::test]
    async fn committed_department_survives_next_department_failure() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let cinema = || Cinema {
            id: "C0159".to_owned(),
            name: "UGC Ciné Cité Les Halles".to_owned(),
            address: Some("7 Place de la Rotonde 75001 Paris".to_owned()),
        };
        upsert_cinemas(&pool, &[cinema()]).await.unwrap();
        sqlx::query("UPDATE cinemas SET lat = 48.8619, lng = 2.3466, geocode_score = 0.9, insee_code = '75101', department = '75'")
            .execute(&pool).await.unwrap();
        upsert_cinemas(&pool, &[cinema()]).await.unwrap();
        let geocoding = sqlx::query_as::<_, (f64, f64, f64, String, String)>(
            "SELECT lat, lng, geocode_score, insee_code, department FROM cinemas WHERE id = 'C0159'"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            geocoding,
            (48.8619, 2.3466, 0.9, "75101".to_owned(), "75".to_owned())
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cinemas")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );

        // Simule un échec SQL pendant le département suivant, après une
        // première écriture : toute cette transaction doit être annulée.
        sqlx::query("CREATE TRIGGER fail_import BEFORE INSERT ON cinemas WHEN NEW.id = 'FAIL' BEGIN SELECT RAISE(ABORT, 'simulated failure'); END")
            .execute(&pool).await.unwrap();
        let next = [
            Cinema {
                id: "NEXT".to_owned(),
                ..cinema()
            },
            Cinema {
                id: "FAIL".to_owned(),
                ..cinema()
            },
        ];
        assert!(upsert_cinemas(&pool, &next).await.is_err());
        let ids = sqlx::query_scalar::<_, String>("SELECT id FROM cinemas")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, vec!["C0159"]);
    }
}
