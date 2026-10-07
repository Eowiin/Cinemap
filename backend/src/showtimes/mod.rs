pub mod allocine;
mod db;
mod mapping;

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, bail};
use bytes::Bytes;
use chrono::{Datelike, Days, NaiveDate, Utc};
use chrono_tz::Europe::Paris;
use reqwest::StatusCode;
use sqlx::SqlitePool;
use tokio::{
    sync::Mutex,
    task::JoinSet,
    time::{Instant, sleep_until},
};
use tracing::{info, warn};

const MAX_CONCURRENT_CINEMAS: usize = 4;
const MAX_CONSECUTIVE_BLOCKS: usize = 5;
const REQUEST_INTERVAL: Duration = Duration::from_nanos(333_333_334);

fn cine_dates(today: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = vec![today, today + Days::new(1), today + Days::new(2)];
    if today.weekday() == chrono::Weekday::Wed {
        dates.push(today + Days::new(6));
    }
    dates
}

#[derive(Clone)]
struct SharedRateLimiter {
    next_request_at: Arc<Mutex<Instant>>,
}

impl SharedRateLimiter {
    fn new() -> Self {
        Self {
            next_request_at: Arc::new(Mutex::new(Instant::now())),
        }
    }

    async fn wait(&self) {
        let mut next_request_at = self.next_request_at.lock().await;
        if *next_request_at > Instant::now() {
            sleep_until(*next_request_at).await;
        }
        *next_request_at = Instant::now() + REQUEST_INTERVAL;
    }
}

#[derive(Default)]
struct CircuitBreaker {
    consecutive_blocks: AtomicUsize,
    tripped: AtomicBool,
}

impl CircuitBreaker {
    fn ensure_active(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.tripped.load(Ordering::Acquire),
            "Coupe-circuit déclenché après {MAX_CONSECUTIVE_BLOCKS} réponses 403/429 consécutives"
        );
        Ok(())
    }

    fn observe(&self, result: &Result<Bytes, reqwest::Error>) {
        match result {
            Ok(_) => self.observe_success(),
            Err(error) => self.observe_status(error.status()),
        }
    }

    fn observe_success(&self) {
        self.consecutive_blocks.store(0, Ordering::Release);
    }

    fn observe_status(&self, status: Option<StatusCode>) {
        if !matches!(
            status,
            Some(StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS)
        ) {
            return;
        }
        let count = self.consecutive_blocks.fetch_add(1, Ordering::AcqRel) + 1;
        if count >= MAX_CONSECUTIVE_BLOCKS {
            self.tripped.store(true, Ordering::Release);
        }
    }

    fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
struct CinemaReport {
    completed_dates: usize,
    error_dates: usize,
    movies: usize,
    showtimes: usize,
}

impl CinemaReport {
    fn succeeded(&self, expected_dates: usize) -> bool {
        self.error_dates == 0 && self.completed_dates == expected_dates
    }
}

#[derive(Debug, Default)]
struct RunReport {
    successful_cinemas: usize,
    failed_cinemas: usize,
    movies: usize,
    showtimes: usize,
    panicked_tasks: usize,
}

async fn visible_cinema_ids(
    pool: &SqlitePool,
    department: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM cinemas WHERE lat IS NOT NULL AND updated_at >= datetime('now', '-14 days') AND (? IS NULL OR department = ?) ORDER BY id",
    )
    .bind(department)
    .bind(department)
    .fetch_all(pool)
    .await?)
}

async fn scrape_cinema(
    pool: SqlitePool,
    client: reqwest::Client,
    cinema_id: String,
    dates: Vec<NaiveDate>,
    rate_limiter: SharedRateLimiter,
    circuit_breaker: Arc<CircuitBreaker>,
) -> CinemaReport {
    let mut report = CinemaReport::default();
    for date in &dates {
        if circuit_breaker.is_tripped() {
            report.error_dates += 1;
            break;
        }
        let date_string = date.format("%Y-%m-%d").to_string();
        let result = allocine::fetch_showtimes(
            &client,
            &cinema_id,
            &date_string,
            &rate_limiter,
            &circuit_breaker,
        )
        .await;
        match result {
            Ok(movies) => {
                let count = allocine::count_showtimes(&movies);
                if let Err(error) =
                    db::save_showtimes(&pool, &cinema_id, &date_string, &movies).await
                {
                    report.error_dates += 1;
                    warn!(cinema = %cinema_id, %date_string, error = %format!("{error:#}"), "Écriture des séances en échec");
                } else {
                    report.completed_dates += 1;
                    report.movies += movies.len();
                    report.showtimes += count;
                }
            }
            Err(error) => {
                report.error_dates += 1;
                warn!(cinema = %cinema_id, %date_string, error = %format!("{error:#}"), "Scraping de ce cinéma/date en échec");
            }
        }
    }
    report
}

fn spawn_cinema_task(
    tasks: &mut JoinSet<CinemaReport>,
    pool: &SqlitePool,
    client: &reqwest::Client,
    cinema_id: String,
    dates: &[NaiveDate],
    rate_limiter: &SharedRateLimiter,
    circuit_breaker: &Arc<CircuitBreaker>,
) {
    tasks.spawn(scrape_cinema(
        pool.clone(),
        client.clone(),
        cinema_id,
        dates.to_vec(),
        rate_limiter.clone(),
        Arc::clone(circuit_breaker),
    ));
}

async fn scrape_cinemas(
    pool: &SqlitePool,
    client: reqwest::Client,
    cinema_ids: Vec<String>,
    dates: Vec<NaiveDate>,
) -> anyhow::Result<()> {
    let mut remaining = VecDeque::from(cinema_ids);
    let rate_limiter = SharedRateLimiter::new();
    let circuit_breaker = Arc::new(CircuitBreaker::default());
    let mut tasks = JoinSet::new();
    let mut report = RunReport::default();

    while tasks.len() < MAX_CONCURRENT_CINEMAS
        && let Some(cinema_id) = remaining.pop_front()
    {
        spawn_cinema_task(
            &mut tasks,
            pool,
            &client,
            cinema_id,
            &dates,
            &rate_limiter,
            &circuit_breaker,
        );
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(cinema_report) => {
                if cinema_report.succeeded(dates.len()) {
                    report.successful_cinemas += 1;
                } else {
                    report.failed_cinemas += 1;
                }
                report.movies += cinema_report.movies;
                report.showtimes += cinema_report.showtimes;
            }
            Err(error) => {
                report.failed_cinemas += 1;
                report.panicked_tasks += 1;
                warn!(error = %error, "Tâche de scraping interrompue");
            }
        }

        if !circuit_breaker.is_tripped()
            && let Some(cinema_id) = remaining.pop_front()
        {
            spawn_cinema_task(
                &mut tasks,
                pool,
                &client,
                cinema_id,
                &dates,
                &rate_limiter,
                &circuit_breaker,
            );
        }
    }

    info!(
        cinemas_ok = report.successful_cinemas,
        cinemas_en_erreur = report.failed_cinemas,
        cinemas_non_lances = remaining.len(),
        films = report.movies,
        seances = report.showtimes,
        taches_interrompues = report.panicked_tasks,
        coupe_circuit = circuit_breaker.is_tripped(),
        "Scraping terminé"
    );
    Ok(())
}

pub async fn scrape(
    pool: &SqlitePool,
    cinema_id: Option<&str>,
    requested_date: Option<&str>,
    department: Option<&str>,
) -> anyhow::Result<()> {
    let dates = if let Some(date) = requested_date {
        vec![
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|| format!("Date invalide : {date}. Format attendu : YYYY-MM-DD"))?,
        ]
    } else {
        let today = Utc::now().with_timezone(&Paris).date_naive();
        cine_dates(today)
    };

    let cinema_ids = if let Some(cinema_id) = cinema_id {
        vec![cinema_id.to_owned()]
    } else {
        visible_cinema_ids(pool, department).await?
    };
    if cinema_ids.is_empty() {
        bail!("Aucun cinéma visible à scraper pour ce département");
    }

    let client = crate::client::build_client()?;
    scrape_cinemas(pool, client, cinema_ids, dates).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cine_dates_returns_three_dates_from_monday() {
        let monday = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(
            cine_dates(monday),
            [
                NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            ]
        );
    }

    #[test]
    fn cine_dates_adds_sixth_day_from_wednesday() {
        let wednesday = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        assert_eq!(
            cine_dates(wednesday),
            [
                NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 8).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 13).unwrap(),
            ]
        );
    }

    #[test]
    fn cine_dates_crosses_the_year_boundary() {
        let wednesday = NaiveDate::from_ymd_opt(2026, 12, 30).unwrap();
        assert_eq!(
            cine_dates(wednesday),
            [
                NaiveDate::from_ymd_opt(2026, 12, 30).unwrap(),
                NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2027, 1, 5).unwrap(),
            ]
        );
    }

    #[test]
    fn circuit_breaker_trips_on_five_consecutive_403_or_429_responses() {
        let breaker = CircuitBreaker::default();
        for _ in 0..4 {
            breaker.observe_status(Some(StatusCode::TOO_MANY_REQUESTS));
        }
        assert!(!breaker.is_tripped());
        breaker.observe_success();
        for _ in 0..4 {
            breaker.observe_status(Some(StatusCode::FORBIDDEN));
        }
        assert!(!breaker.is_tripped());
        breaker.observe_status(Some(StatusCode::TOO_MANY_REQUESTS));
        assert!(breaker.is_tripped());
        assert!(breaker.ensure_active().is_err());
    }

    #[tokio::test]
    async fn visible_cinema_query_filters_location_age_and_department() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, lat, department, updated_at) VALUES
             ('PARIS', 'Paris', 'paris', 48.0, '75', datetime('now')),
             ('LYON', 'Lyon', 'lyon', 45.0, '69', datetime('now')),
             ('NO_GEO', 'Sans GPS', 'sans gps', NULL, '75', datetime('now')),
             ('OLD', 'Ancien', 'ancien', 48.0, '75', '2000-01-01 00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            visible_cinema_ids(&pool, Some("75")).await.unwrap(),
            ["PARIS"]
        );
        assert_eq!(
            visible_cinema_ids(&pool, None).await.unwrap(),
            ["LYON", "PARIS"]
        );
    }
}
