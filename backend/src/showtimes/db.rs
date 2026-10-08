use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::{Sqlite, Transaction};

use crate::text::normalize;

use super::{
    allocine::MovieResult,
    mapping::{booking_url, formats, full_name, runtime_minutes, version},
};

/// Avant cette heure, une séance appartient au jour ciné **précédent** (`API.md`,
/// « jour ciné »). AlloCiné renvoie les séances d'après minuit deux fois, avec le même ID :
/// en fin de programme de J-1 (`starts_at` = J à 00:15) et en début de programme de J.
/// On ne la garde qu'en J-1 (vu sur G02BG le 2026-10-08 : contrainte UNIQUE sur `showtimes.id`).
const CINE_DAY_STARTS_AT: &str = "05:00:00";

/// Remplace les séances du couple (cinéma, date) et renvoie le nombre de séances écrites.
pub(super) async fn save_showtimes(
    pool: &SqlitePool,
    cinema_id: &str,
    date: &str,
    movies: &[MovieResult],
) -> Result<usize> {
    let day_start = format!("{date}T{CINE_DAY_STARTS_AT}");
    let mut tx = pool.begin().await?;

    for entry in movies {
        upsert_movie(&mut tx, &entry.movie).await?;
    }

    sqlx::query("DELETE FROM showtimes WHERE cinema_id = ? AND date = ?")
        .bind(cinema_id)
        .bind(date)
        .execute(&mut *tx)
        .await?;

    let mut written = 0;
    for entry in movies {
        for showtime in entry.showtimes.values().flatten() {
            if showtime.starts_at < day_start {
                continue; // déjà enregistrée avec le jour ciné précédent
            }
            if insert_showtime(&mut tx, cinema_id, date, entry, showtime).await? {
                written += 1;
            }
        }
    }

    tx.commit().await?;
    Ok(written)
}

/// Ouvre une ligne `scrape_runs` et renvoie son `id`. Tant que `finish_run` n'a pas été
/// appelé, `finished_at` reste `NULL` : un run interrompu (crash, Ctrl+C) se voit.
pub(super) async fn start_run(pool: &SqlitePool, kind: &str) -> Result<i64> {
    let result =
        sqlx::query("INSERT INTO scrape_runs (kind, started_at) VALUES (?, datetime('now'))")
            .bind(kind)
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

/// Ferme **cette** ligne. Les compteurs sont des couples (cinéma, date) : c'est l'unité
/// d'écriture (une transaction), et un cinéma peut réussir J et rater J+1.
pub(super) async fn finish_run(
    pool: &SqlitePool,
    run_id: i64,
    ok_count: usize,
    error_count: usize,
) -> Result<()> {
    sqlx::query(
        "UPDATE scrape_runs SET finished_at = datetime('now'), ok_count = ?, error_count = ? WHERE id = ?",
    )
    .bind(i64::try_from(ok_count)?)
    .bind(i64::try_from(error_count)?)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Supprime les séances des jours ciné passés (`date < today`, `today` = aujourd'hui à
/// Paris) et renvoie leur nombre. Les films sans séance restent : TMDB a pu les enrichir.
pub(super) async fn purge_past_showtimes(pool: &SqlitePool, today: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM showtimes WHERE date < ?")
        .bind(today)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// État de la table `showtimes` en fin de run (après la purge), pour le rapport.
#[derive(Debug, PartialEq, sqlx::FromRow)]
pub(super) struct ShowtimesSummary {
    pub showtimes: i64,
    pub movies: i64,
    pub cinemas: i64,
    pub vf: i64,
    pub vo: i64,
    pub vost: i64,
    pub without_booking_url: i64,
}

/// Une seule requête, comme `import_report` dans `cinemas/mod.rs` : `sum(condition)`
/// compte les lignes où la condition vaut 1 (`coalesce` car `sum` d'une table vide = `NULL`).
pub(super) async fn showtimes_summary(pool: &SqlitePool) -> Result<ShowtimesSummary> {
    Ok(sqlx::query_as(
        "SELECT
            count(*) AS showtimes,
            count(DISTINCT movie_id) AS movies,
            count(DISTINCT cinema_id) AS cinemas,
            coalesce(sum(version = 'VF'), 0) AS vf,
            coalesce(sum(version = 'VO'), 0) AS vo,
            coalesce(sum(version = 'VOST'), 0) AS vost,
            coalesce(sum(booking_url IS NULL), 0) AS without_booking_url
         FROM showtimes",
    )
    .fetch_one(pool)
    .await?)
}

struct MovieValues<'a> {
    id: i64,
    title: &'a str,
    title_search: String,
    original_title: Option<&'a str>,
    poster_url: Option<&'a str>,
    synopsis: Option<&'a str>,
    runtime_min: Option<i64>,
    release_date: Option<&'a str>,
    production_year: Option<i64>,
    certificate: Option<&'a str>,
    genres: String,
    directors: String,
    cast_members: String,
    countries: String,
    user_rating: Option<f64>,
}

fn movie_values(movie: &super::allocine::Movie) -> Result<MovieValues<'_>> {
    let genres = movie
        .genres
        .iter()
        .filter_map(|genre| genre.translate.as_deref())
        .collect::<Vec<_>>();
    let countries = movie
        .countries
        .iter()
        .filter_map(|country| country.localized_name.as_deref())
        .collect::<Vec<_>>();
    let directors = movie
        .credits
        .iter()
        .filter(|credit| {
            credit
                .position
                .as_ref()
                .and_then(|position| position.name.as_deref())
                == Some("DIRECTOR")
        })
        .filter_map(|credit| credit.person.as_ref())
        .filter_map(|person| full_name(person.first_name.as_deref(), person.last_name.as_deref()))
        .collect::<Vec<_>>();
    let cast_members = movie
        .cast
        .edges
        .iter()
        .filter_map(|edge| edge.node.as_ref())
        .filter_map(|node| {
            let actor = node.actor.as_ref()?;
            Some(serde_json::json!({
                "name": full_name(actor.first_name.as_deref(), actor.last_name.as_deref())?,
                "role": node.role,
            }))
        })
        .take(10)
        .collect::<Vec<_>>();
    let release = movie.releases.first();

    Ok(MovieValues {
        id: movie.internal_id,
        title: &movie.title,
        title_search: normalize(&movie.title),
        original_title: movie.original_title.as_deref(),
        poster_url: movie
            .poster
            .as_ref()
            .and_then(|poster| poster.url.as_deref()),
        synopsis: movie.synopsis.as_deref(),
        runtime_min: movie
            .runtime
            .as_deref()
            .and_then(runtime_minutes)
            .map(i64::from),
        release_date: release
            .and_then(|release| release.release_date.as_ref())
            .and_then(|date| date.date.as_deref()),
        production_year: movie.data.production_year,
        certificate: release
            .and_then(|release| release.certificate.as_ref())
            .and_then(|certificate| certificate.label.as_deref()),
        genres: serde_json::to_string(&genres)?,
        directors: serde_json::to_string(&directors)?,
        cast_members: serde_json::to_string(&cast_members)?,
        countries: serde_json::to_string(&countries)?,
        user_rating: movie
            .stats
            .user_rating
            .as_ref()
            .and_then(|rating| rating.score),
    })
}

async fn upsert_movie(
    tx: &mut Transaction<'_, Sqlite>,
    movie: &super::allocine::Movie,
) -> Result<()> {
    let values = movie_values(movie)?;
    sqlx::query(
        r#"INSERT INTO movies (
               id, title, title_search, original_title, poster_url, synopsis, runtime_min,
               release_date, production_year, certificate, genres, directors, cast_members,
               countries, user_rating, updated_at
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
           ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               title_search = excluded.title_search,
               original_title = excluded.original_title,
               poster_url = excluded.poster_url,
               synopsis = excluded.synopsis,
               runtime_min = excluded.runtime_min,
               release_date = excluded.release_date,
               production_year = excluded.production_year,
               certificate = excluded.certificate,
               genres = excluded.genres,
               directors = excluded.directors,
               cast_members = excluded.cast_members,
               countries = excluded.countries,
               user_rating = excluded.user_rating,
               updated_at = excluded.updated_at"#,
    )
    .bind(values.id)
    .bind(values.title)
    .bind(values.title_search)
    .bind(values.original_title)
    .bind(values.poster_url)
    .bind(values.synopsis)
    .bind(values.runtime_min)
    .bind(values.release_date)
    .bind(values.production_year)
    .bind(values.certificate)
    .bind(values.genres)
    .bind(values.directors)
    .bind(values.cast_members)
    .bind(values.countries)
    .bind(values.user_rating)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_showtime(
    tx: &mut Transaction<'_, Sqlite>,
    cinema_id: &str,
    date: &str,
    entry: &MovieResult,
    showtime: &super::allocine::Showtime,
) -> Result<bool> {
    let Some(mapped_version) = version(showtime, &entry.movie.languages) else {
        tracing::warn!(
            cinema_id,
            date,
            showtime_id = showtime.internal_id,
            diffusion_version = %showtime.diffusion_version,
            "Séance ignorée : version AlloCiné inconnue"
        );
        return Ok(false);
    };

    let formats = serde_json::to_string(&formats(showtime))?;
    sqlx::query(
        "INSERT INTO showtimes (id, cinema_id, movie_id, date, starts_at, version, formats, booking_url) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(showtime.internal_id.to_string())
    .bind(cinema_id)
    .bind(entry.movie.internal_id)
    .bind(date)
    .bind(&showtime.starts_at)
    .bind(mapped_version.as_str())
    .bind(formats)
    .bind(booking_url(showtime))
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::showtimes::allocine::Response;

    const C0159_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p1.json");

    async fn memory_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    async fn insert_showtime_row(pool: &SqlitePool, id: &str, date: &str, version: &str) {
        sqlx::query(
            "INSERT INTO showtimes (id, cinema_id, movie_id, date, starts_at, version, booking_url)
             VALUES (?, 'C1', 1, ?, ?, ?, CASE WHEN ? = 'VO' THEN NULL ELSE 'https://x' END)",
        )
        .bind(id)
        .bind(date)
        .bind(format!("{date}T20:00:00"))
        .bind(version)
        .bind(version)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn pool_with_one_cinema_and_movie() -> SqlitePool {
        let pool = memory_pool().await;
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at) VALUES ('C1', 'C', 'c', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO movies (id, title, title_search, updated_at) VALUES (1, 'F', 'f', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn after_midnight_showtime_stays_on_previous_cine_day() {
        let pool = memory_pool().await;
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at) VALUES ('C1', 'C', 'c', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Même séance (même ID) renvoyée pour le 9 et pour le 10, comme sur G02BG.
        let movies = serde_json::from_str::<Response>(
            r#"{"error":false,"results":[{"movie":{"internalId":1,"title":"Film"},
                "showtimes":{"multiple":[{"internalId":42,"startsAt":"2026-10-10T00:15:00",
                "tags":["Localization.Version.French"]}]}}]}"#,
        )
        .unwrap()
        .results;

        assert_eq!(
            save_showtimes(&pool, "C1", "2026-10-09", &movies)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            save_showtimes(&pool, "C1", "2026-10-10", &movies)
                .await
                .unwrap(),
            0
        );

        let date: String = sqlx::query_scalar("SELECT date FROM showtimes WHERE id = '42'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(date, "2026-10-09");
    }

    #[tokio::test]
    async fn purge_removes_only_past_dates() {
        let pool = pool_with_one_cinema_and_movie().await;
        insert_showtime_row(&pool, "yesterday", "2026-10-07", "VF").await;
        insert_showtime_row(&pool, "today", "2026-10-08", "VF").await;
        insert_showtime_row(&pool, "tomorrow", "2026-10-09", "VF").await;

        let purged = purge_past_showtimes(&pool, "2026-10-08").await.unwrap();

        let remaining: Vec<String> = sqlx::query_scalar("SELECT id FROM showtimes ORDER BY date")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(purged, 1);
        assert_eq!(remaining, ["today", "tomorrow"]);
    }

    #[tokio::test]
    async fn run_stays_open_until_finished() {
        let pool = memory_pool().await;
        let first = start_run(&pool, "showtimes").await.unwrap();
        let second = start_run(&pool, "showtimes").await.unwrap();
        finish_run(&pool, second, 12, 3).await.unwrap();

        let rows: Vec<(i64, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, finished_at, ok_count, error_count FROM scrape_runs ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].0, rows[0].1.is_none()), (first, true));
        assert_eq!((rows[1].0, rows[1].1.is_some()), (second, true));
        assert_eq!((rows[1].2, rows[1].3), (12, 3));
    }

    #[tokio::test]
    async fn summary_counts_versions_and_missing_links() {
        let pool = pool_with_one_cinema_and_movie().await;
        assert_eq!(
            showtimes_summary(&pool).await.unwrap(),
            ShowtimesSummary {
                showtimes: 0,
                movies: 0,
                cinemas: 0,
                vf: 0,
                vo: 0,
                vost: 0,
                without_booking_url: 0,
            }
        );
        insert_showtime_row(&pool, "a", "2026-10-08", "VF").await;
        insert_showtime_row(&pool, "b", "2026-10-08", "VOST").await;
        insert_showtime_row(&pool, "c", "2026-10-09", "VO").await;

        assert_eq!(
            showtimes_summary(&pool).await.unwrap(),
            ShowtimesSummary {
                showtimes: 3,
                movies: 1,
                cinemas: 1,
                vf: 1,
                vo: 1,
                vost: 1,
                without_booking_url: 1,
            }
        );
    }

    #[tokio::test]
    async fn save_showtimes_replaces_date_and_preserves_tmdb_data() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at) VALUES ('C0159', 'UGC', 'ugc', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut movies = serde_json::from_str::<Response>(C0159_PAGE_1)
            .unwrap()
            .results;
        let (movie_id, removed_id) = movies
            .iter()
            .find_map(|entry| {
                entry.showtimes.values().flatten().find_map(|showtime| {
                    version(showtime, &entry.movie.languages)
                        .is_some()
                        .then_some((entry.movie.internal_id, showtime.internal_id))
                })
            })
            .expect("la fixture doit contenir une séance avec une version reconnue");

        save_showtimes(&pool, "C0159", "2026-10-06", &movies)
            .await
            .unwrap();
        sqlx::query("UPDATE movies SET tmdb_id = 12345 WHERE id = ?")
            .bind(movie_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO showtimes (id, cinema_id, movie_id, date, starts_at, version) VALUES (?, 'C0159', ?, '2026-10-07', '2026-10-07T20:00:00', 'VO')",
        )
        .bind(format!("other-date-{removed_id}"))
        .bind(movie_id)
        .execute(&pool)
        .await
        .unwrap();

        for entry in &mut movies {
            for screenings in entry.showtimes.values_mut() {
                screenings.retain(|showtime| showtime.internal_id != removed_id);
            }
        }
        save_showtimes(&pool, "C0159", "2026-10-06", &movies)
            .await
            .unwrap();

        let removed_count: i64 = sqlx::query_scalar("SELECT count(*) FROM showtimes WHERE id = ?")
            .bind(removed_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        let other_date_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM showtimes WHERE id = ? AND date = '2026-10-07'",
        )
        .bind(format!("other-date-{removed_id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let tmdb_id: Option<i64> = sqlx::query_scalar("SELECT tmdb_id FROM movies WHERE id = ?")
            .bind(movie_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(removed_count, 0);
        assert_eq!(other_date_count, 1);
        assert_eq!(tmdb_id, Some(12345));
    }
}
