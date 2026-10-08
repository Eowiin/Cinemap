//! Enrichissement des films par TMDB : image de fond, bande-annonce, note.
//!
//! Seuls les films qui ont une séance à venir sont traités, et chacun au plus une fois
//! par semaine (`tmdb_synced_at`), sauf `--all`. Un film déjà croisé (`tmdb_id` connu)
//! n'est plus recherché : sa fiche est seulement relue (note et bande-annonce bougent).

pub mod api;
mod matching;

use std::time::Instant;

use anyhow::{Context, bail};
use reqwest::StatusCode;
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::time::{DATE_FORMAT, paris_today};
use api::Tmdb;
use matching::{Enrichment, best_match, enrichment};

const RESYNC_AFTER: &str = "-7 days";

#[derive(Debug, sqlx::FromRow)]
struct MovieToSync {
    id: i64,
    title: String,
    original_title: Option<String>,
    production_year: Option<i64>,
    tmdb_id: Option<i64>,
}

enum Outcome {
    Matched,
    NotFound,
}

#[derive(Debug, Default)]
struct Report {
    matched: usize,
    not_found: usize,
    errors: usize,
}

pub async fn sync(pool: &SqlitePool, all: bool) -> anyhow::Result<()> {
    let key = std::env::var("TMDB_API_KEY").context(
        "Variable TMDB_API_KEY introuvable : crée une clé sur themoviedb.org (Paramètres > API) et ajoute-la à backend/.env",
    )?;
    let mut tmdb = Tmdb::new(crate::client::build_client()?, key);
    let today = paris_today().format(DATE_FORMAT).to_string();
    let movies = movies_to_sync(pool, &today, all).await?;
    info!(films = movies.len(), "Enrichissement TMDB : début");

    let started = Instant::now();
    let run_id = start_run(pool).await?;
    let mut report = Report::default();
    for movie in &movies {
        match sync_movie(pool, &mut tmdb, movie).await {
            Ok(Outcome::Matched) => report.matched += 1,
            Ok(Outcome::NotFound) => report.not_found += 1,
            Err(error) if is_unauthorized(&error) => {
                finish_run(pool, run_id, &report).await?;
                return Err(error.context("Clé TMDB refusée (401) : vérifie TMDB_API_KEY"));
            }
            Err(error) => {
                report.errors += 1;
                warn!(film = movie.id, titre = %movie.title, error = format!("{error:#}"), "Film non enrichi");
            }
        }
    }
    finish_run(pool, run_id, &report).await?;

    info!(
        croises = report.matched,
        introuvables = report.not_found,
        erreurs = report.errors,
        duree_s = started.elapsed().as_secs(),
        "Enrichissement TMDB terminé"
    );
    if !movies.is_empty() && report.errors == movies.len() {
        bail!("Aucun film n'a pu être enrichi");
    }
    Ok(())
}

async fn movies_to_sync(
    pool: &SqlitePool,
    today: &str,
    all: bool,
) -> anyhow::Result<Vec<MovieToSync>> {
    Ok(sqlx::query_as(
        "SELECT m.id, m.title, m.original_title, m.production_year, m.tmdb_id
         FROM movies m
         WHERE EXISTS (SELECT 1 FROM showtimes s WHERE s.movie_id = m.id AND s.date >= ?1)
           AND (?2 OR m.tmdb_synced_at IS NULL OR m.tmdb_synced_at < datetime('now', ?3))
         ORDER BY m.id",
    )
    .bind(today)
    .bind(all)
    .bind(RESYNC_AFTER)
    .fetch_all(pool)
    .await?)
}

async fn sync_movie(
    pool: &SqlitePool,
    tmdb: &mut Tmdb,
    movie: &MovieToSync,
) -> anyhow::Result<Outcome> {
    let tmdb_id = match movie.tmdb_id {
        Some(id) => Some(id),
        None => find_tmdb_id(tmdb, movie).await?,
    };
    let Some(tmdb_id) = tmdb_id else {
        mark_synced(pool, movie.id).await?;
        return Ok(Outcome::NotFound);
    };
    let details = tmdb.details(tmdb_id).await?;
    save_enrichment(pool, movie.id, &enrichment(&details)).await?;
    Ok(Outcome::Matched)
}

/// Recherche par titre original, puis par titre français s'il diffère.
async fn find_tmdb_id(tmdb: &mut Tmdb, movie: &MovieToSync) -> anyhow::Result<Option<i64>> {
    let original = movie.original_title.as_deref().unwrap_or(&movie.title);
    let titles = [original, movie.title.as_str()];

    let candidates = tmdb.search(original).await?;
    if let Some(found) = best_match(&candidates, &titles, movie.production_year) {
        return Ok(Some(found.id));
    }
    if movie.title != original {
        let candidates = tmdb.search(&movie.title).await?;
        if let Some(found) = best_match(&candidates, &titles, movie.production_year) {
            return Ok(Some(found.id));
        }
    }
    Ok(None)
}

async fn save_enrichment(pool: &SqlitePool, movie_id: i64, e: &Enrichment) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE movies
         SET tmdb_id = ?, backdrop_url = ?, trailer_url = ?, rating = ?, tmdb_synced_at = datetime('now')
         WHERE id = ?",
    )
    .bind(e.tmdb_id)
    .bind(&e.backdrop_url)
    .bind(&e.trailer_url)
    .bind(e.rating)
    .bind(movie_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Film introuvable sur TMDB : on note la tentative pour ne pas rechercher chaque nuit.
async fn mark_synced(pool: &SqlitePool, movie_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE movies SET tmdb_synced_at = datetime('now') WHERE id = ?")
        .bind(movie_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn start_run(pool: &SqlitePool) -> anyhow::Result<i64> {
    let result =
        sqlx::query("INSERT INTO scrape_runs (kind, started_at) VALUES ('tmdb', datetime('now'))")
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

async fn finish_run(pool: &SqlitePool, run_id: i64, report: &Report) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE scrape_runs SET finished_at = datetime('now'), ok_count = ?, error_count = ? WHERE id = ?",
    )
    .bind((report.matched + report.not_found) as i64)
    .bind(report.errors as i64)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn is_unauthorized(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .and_then(reqwest::Error::status)
            == Some(StatusCode::UNAUTHORIZED)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO cinemas (id, name, name_search, updated_at) VALUES ('C1', 'C', 'c', datetime('now'));
             INSERT INTO movies (id, title, title_search, tmdb_synced_at, updated_at) VALUES
                 (1, 'Jamais vu', 'jamais vu', NULL, datetime('now')),
                 (2, 'Vu hier', 'vu hier', datetime('now', '-1 day'), datetime('now')),
                 (3, 'Vu il y a longtemps', 'vu il y a longtemps', datetime('now', '-30 days'), datetime('now')),
                 (4, 'Plus à l''affiche', 'plus a l''affiche', NULL, datetime('now'));
             INSERT INTO showtimes (id, cinema_id, movie_id, date, starts_at, version) VALUES
                 ('s1', 'C1', 1, '2099-01-01', '2099-01-01T20:00:00', 'VF'),
                 ('s2', 'C1', 2, '2099-01-01', '2099-01-01T20:00:00', 'VF'),
                 ('s3', 'C1', 3, '2099-01-01', '2099-01-01T20:00:00', 'VF'),
                 ('s4', 'C1', 4, '2000-01-01', '2000-01-01T20:00:00', 'VF');",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn ids(pool: &SqlitePool, all: bool) -> Vec<i64> {
        movies_to_sync(pool, "2026-10-08", all)
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    #[tokio::test]
    async fn selects_upcoming_movies_not_synced_this_week() {
        let pool = pool().await;
        assert_eq!(ids(&pool, false).await, [1, 3]);
        assert_eq!(ids(&pool, true).await, [1, 2, 3]);
    }

    #[tokio::test]
    async fn save_and_mark_update_the_sync_date() {
        let pool = pool().await;
        save_enrichment(
            &pool,
            1,
            &Enrichment {
                tmdb_id: 99,
                backdrop_url: Some("https://b".into()),
                trailer_url: None,
                rating: Some(7.5),
            },
        )
        .await
        .unwrap();
        mark_synced(&pool, 3).await.unwrap();
        assert!(ids(&pool, false).await.is_empty());
        let (tmdb_id, rating): (Option<i64>, Option<f64>) =
            sqlx::query_as("SELECT tmdb_id, rating FROM movies WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((tmdb_id, rating), (Some(99), Some(7.5)));
    }
}
