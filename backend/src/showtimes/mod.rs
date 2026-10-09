pub mod allocine;
mod db;
mod mapping;

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, bail};
use bytes::Bytes;
use chrono::{Datelike, Days, NaiveDate};
use reqwest::StatusCode;
use sqlx::SqlitePool;
use tokio::{
    task::JoinSet,
    time::{Instant, sleep_until},
};
use tracing::{info, warn};

use crate::time::{DATE_FORMAT, paris_today};

const MAX_CONCURRENT_CINEMAS: usize = 4;
/// Refus (403/429) d'affilée, chacun après une pause, sans aucune réponse correcte entre
/// eux : 15 min de refus continus. On arrête le run.
const MAX_CONSECUTIVE_BLOCKS: u32 = 3;
/// Rythme de départ et rythme maximal par défaut : 3 requêtes/s (`scrape --interval-ms`
/// pour un rythme plus lent).
pub const MIN_INTERVAL: Duration = Duration::from_nanos(333_333_334);
/// Rythme minimal après ralentissements : une requête toutes les 5 s.
const MAX_INTERVAL: Duration = Duration::from_secs(5);
/// Pause de tous les workers après un refus. Vu depuis le VPS (run du 2026-10-08, 19 blocages
/// identiques) : AlloCiné bloque l'IP plus de 3 min 30 et moins de 7 min 30. Des essais
/// plus rapprochés (30 s, 1 min, 2 min) étaient tous refusés.
const BLOCK_PAUSE: Duration = Duration::from_secs(5 * 60);
/// Réponses correctes d'affilée avant d'accélérer d'un cran.
const SPEEDUP_AFTER: u32 = 20;

/// `days` jours consécutifs à partir d'aujourd'hui (`scrape --days`).
fn next_days(today: NaiveDate, days: u32) -> Vec<NaiveDate> {
    (0..u64::from(days)).map(|n| today + Days::new(n)).collect()
}

fn cine_dates(today: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = vec![today, today + Days::new(1), today + Days::new(2)];
    if today.weekday() == chrono::Weekday::Wed {
        dates.push(today + Days::new(6));
    }
    dates
}

fn is_block(status: Option<StatusCode>) -> bool {
    matches!(
        status,
        Some(StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS)
    )
}

/// Rythme des requêtes, partagé par tous les workers (logique pure : `now` est passé en
/// paramètre, ce qui rend les tests déterministes).
///
/// Même principe que le contrôle de congestion de TCP (AIMD) : on part vite, on divise le
/// débit par deux à chaque refus (avec une pause), et on le remonte doucement tant que
/// tout passe (+25 % toutes les `SPEEDUP_AFTER` réponses correctes), jamais au-delà du
/// rythme de départ.
#[derive(Debug)]
struct Pace {
    /// Intervalle de départ, et plancher quand on accélère.
    min_interval: Duration,
    interval: Duration,
    next_slot: Instant,
    paused_until: Instant,
    successes: u32,
    /// Refus d'affilée (hors pause), remis à zéro par une réponse correcte hors pause.
    consecutive_blocks: u32,
    slowdowns: usize,
}

impl Pace {
    fn new(now: Instant, min_interval: Duration) -> Self {
        Self {
            min_interval,
            interval: min_interval,
            next_slot: now,
            paused_until: now,
            successes: 0,
            consecutive_blocks: 0,
            slowdowns: 0,
        }
    }

    /// Réserve le prochain créneau d'envoi : jamais pendant une pause, et espacé de
    /// `interval` du créneau précédent.
    fn reserve(&mut self, now: Instant) -> Instant {
        let slot = self.next_slot.max(now).max(self.paused_until);
        self.next_slot = slot + self.interval;
        slot
    }

    fn is_paused(&self, now: Instant) -> bool {
        now < self.paused_until
    }

    /// Une réponse correcte reçue pendant une pause vient d'une requête partie avant le refus :
    /// elle ne prouve pas que le blocage est levé, on l'ignore.
    fn on_success(&mut self, now: Instant) {
        if self.is_paused(now) {
            return;
        }
        self.consecutive_blocks = 0;
        self.successes += 1;
        if self.successes >= SPEEDUP_AFTER {
            self.successes = 0;
            self.interval = (self.interval * 4 / 5).max(self.min_interval);
        }
    }

    /// Renvoie le nombre de refus d'affilée, ou `None` si le refus arrive pendant une pause :
    /// c'est la réponse d'une requête partie avant le ralentissement, déjà pris en compte.
    fn on_block(&mut self, now: Instant) -> Option<u32> {
        if self.is_paused(now) {
            return None;
        }
        self.consecutive_blocks += 1;
        self.interval = (self.interval * 2).min(MAX_INTERVAL.max(self.min_interval));
        self.paused_until = now + BLOCK_PAUSE;
        self.next_slot = self.next_slot.max(self.paused_until);
        self.successes = 0;
        self.slowdowns += 1;
        Some(self.consecutive_blocks)
    }
}

#[derive(Clone)]
struct SharedRateLimiter {
    // Mutex de std (pas de tokio) : jamais tenu pendant un `.await`, seulement le temps d'un calcul.
    pace: Arc<Mutex<Pace>>,
}

impl SharedRateLimiter {
    fn new(min_interval: Duration) -> Self {
        Self {
            pace: Arc::new(Mutex::new(Pace::new(Instant::now(), min_interval))),
        }
    }

    fn pace(&self) -> MutexGuard<'_, Pace> {
        // Un panic pendant un calcul de quelques lignes ne laisse pas d'état incohérent.
        self.pace
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn wait(&self) {
        loop {
            let slot = self.pace().reserve(Instant::now());
            sleep_until(slot).await;
            // Une pause a pu être décidée pendant l'attente : on reprend un créneau après elle.
            if !self.pace().is_paused(Instant::now()) {
                return;
            }
        }
    }

    /// Ajuste le rythme selon la réponse ; après un nouveau refus (hors pause), renvoie le
    /// nombre de refus d'affilée.
    fn observe(&self, status: Result<(), Option<StatusCode>>) -> Option<u32> {
        let now = Instant::now();
        let mut pace = self.pace();
        match status {
            Ok(()) => {
                pace.on_success(now);
                None
            }
            Err(status) if is_block(status) => {
                let blocks = pace.on_block(now)?;
                warn!(
                    refus_d_affilee = blocks,
                    pause_s = BLOCK_PAUSE.as_secs(),
                    intervalle_ms = pace.interval.as_millis(),
                    "AlloCiné refuse des requêtes : pause, puis rythme divisé par deux"
                );
                Some(blocks)
            }
            Err(_) => None,
        }
    }

    fn summary(&self) -> (usize, Duration) {
        let pace = self.pace();
        (pace.slowdowns, pace.interval)
    }
}

#[derive(Default)]
struct CircuitBreaker {
    tripped: AtomicBool,
}

impl CircuitBreaker {
    fn ensure_active(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.tripped.load(Ordering::Acquire),
            "Coupe-circuit déclenché après {MAX_CONSECUTIVE_BLOCKS} refus 403/429 consécutifs malgré les pauses"
        );
        Ok(())
    }

    /// `consecutive_blocks` : compté par `Pace`, qui sait quels refus et quelles réponses
    /// correctes comptent (ceux reçus pendant une pause, non).
    fn observe_blocks(&self, consecutive_blocks: u32) {
        if consecutive_blocks >= MAX_CONSECUTIVE_BLOCKS {
            self.tripped.store(true, Ordering::Release);
        }
    }

    fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Acquire)
    }
}

/// Branche la réponse d'une requête AlloCiné sur le limiteur et le coupe-circuit.
fn observe_response(
    result: &Result<Bytes, reqwest::Error>,
    rate_limiter: &SharedRateLimiter,
    circuit_breaker: &CircuitBreaker,
) {
    let status = match result {
        Ok(_) => Ok(()),
        Err(error) => Err(error.status()),
    };
    if let Some(blocks) = rate_limiter.observe(status) {
        circuit_breaker.observe_blocks(blocks);
    }
}

/// Résultat d'un cinéma : les dates non terminées (erreur ou coupe-circuit) se déduisent
/// de `dates.len() - completed_dates`.
#[derive(Debug, Default)]
struct CinemaReport {
    completed_dates: usize,
    showtimes: usize,
    /// Dates vidées sans requête : AlloCiné avait déjà dit qu'il n'y aurait rien.
    skipped_dates: usize,
}

/// Compteurs du run. `ok_dates` / `error_dates` sont des couples (cinéma, date), comme
/// `scrape_runs.ok_count` / `error_count`.
#[derive(Debug, Default)]
struct RunReport {
    successful_cinemas: usize,
    failed_cinemas: usize,
    not_started_cinemas: usize,
    ok_dates: usize,
    error_dates: usize,
    showtimes: usize,
    skipped_dates: usize,
    panicked_tasks: usize,
    circuit_breaker_tripped: bool,
    slowdowns: usize,
    final_interval: Duration,
}

impl RunReport {
    fn add_cinema(&mut self, cinema: &CinemaReport, expected_dates: usize) {
        let failed_dates = expected_dates - cinema.completed_dates;
        if failed_dates == 0 {
            self.successful_cinemas += 1;
        } else {
            self.failed_cinemas += 1;
        }
        self.ok_dates += cinema.completed_dates;
        self.error_dates += failed_dates;
        self.showtimes += cinema.showtimes;
        self.skipped_dates += cinema.skipped_dates;
    }
}

/// Cinémas visibles, tous ou seulement ceux de `departments` s'il n'est pas vide.
async fn visible_cinema_ids(
    pool: &SqlitePool,
    departments: &[String],
) -> anyhow::Result<Vec<String>> {
    // Liste de longueur variable : passée en JSON et dépliée par `json_each`.
    let departments = serde_json::to_string(departments)?;
    Ok(sqlx::query_scalar(
        "SELECT id FROM visible_cinemas
         WHERE ? = '[]' OR department IN (SELECT value FROM json_each(?))
         ORDER BY id",
    )
    .bind(&departments)
    .bind(&departments)
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
    // Jour vide annoncé par AlloCiné : rien avant `empty_until` (exclu), inutile de demander.
    let mut empty_until: Option<NaiveDate> = None;
    for date in &dates {
        if circuit_breaker.is_tripped() {
            break;
        }
        let date_string = date.format(DATE_FORMAT).to_string();
        let movies = if empty_until.is_some_and(|until| *date < until) {
            report.skipped_dates += 1;
            Vec::new()
        } else {
            let result = allocine::fetch_showtimes(
                &client,
                &cinema_id,
                &date_string,
                &rate_limiter,
                &circuit_breaker,
            )
            .await;
            match result {
                Ok(allocine::Day::Showtimes(movies)) => movies,
                Ok(allocine::Day::Empty { empty_until: until }) => {
                    empty_until = until;
                    Vec::new()
                }
                Err(error) => {
                    warn!(cinema = %cinema_id, %date_string, error = %format!("{error:#}"), "Scraping de ce cinéma/date en échec");
                    continue;
                }
            }
        };
        // Même vide, on enregistre : ça efface les séances d'un ancien run pour ce jour.
        match db::save_showtimes(&pool, &cinema_id, &date_string, &movies).await {
            Ok(written) => {
                report.completed_dates += 1;
                report.showtimes += written;
            }
            Err(error) => {
                warn!(cinema = %cinema_id, %date_string, error = %format!("{error:#}"), "Écriture des séances en échec");
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
    dates: &[NaiveDate],
    min_interval: Duration,
) -> RunReport {
    let mut remaining = VecDeque::from(cinema_ids);
    let rate_limiter = SharedRateLimiter::new(min_interval);
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
            dates,
            &rate_limiter,
            &circuit_breaker,
        );
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(cinema_report) => report.add_cinema(&cinema_report, dates.len()),
            Err(error) => {
                report.failed_cinemas += 1;
                report.error_dates += dates.len();
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
                dates,
                &rate_limiter,
                &circuit_breaker,
            );
        }
    }

    // Coupe-circuit : les cinémas jamais lancés comptent en erreur (données pas rafraîchies).
    report.not_started_cinemas = remaining.len();
    report.error_dates += remaining.len() * dates.len();
    report.circuit_breaker_tripped = circuit_breaker.is_tripped();
    (report.slowdowns, report.final_interval) = rate_limiter.summary();
    report
}

fn log_run_report(
    report: &RunReport,
    summary: &db::ShowtimesSummary,
    purged: u64,
    elapsed: Duration,
) {
    info!(
        cinemas_ok = report.successful_cinemas,
        cinemas_en_erreur = report.failed_cinemas,
        cinemas_non_lances = report.not_started_cinemas,
        couples_ok = report.ok_dates,
        couples_en_erreur = report.error_dates,
        seances_ecrites = report.showtimes,
        dates_sautees = report.skipped_dates,
        ralentissements = report.slowdowns,
        intervalle_final_ms = report.final_interval.as_millis(),
        taches_interrompues = report.panicked_tasks,
        coupe_circuit = report.circuit_breaker_tripped,
        seances_passees_purgees = purged,
        duree_s = elapsed.as_secs(),
        "Scraping terminé"
    );
    info!(
        seances = summary.showtimes,
        films = summary.movies,
        cinemas = summary.cinemas,
        vf = summary.vf,
        vo = summary.vo,
        vost = summary.vost,
        sans_lien = summary.without_booking_url,
        "État de la base après le run"
    );
}

pub async fn scrape(
    pool: &SqlitePool,
    cinema_id: Option<&str>,
    requested_date: Option<&str>,
    departments: &[String],
    days: Option<u32>,
    nightly: bool,
    min_interval: Duration,
) -> anyhow::Result<()> {
    let today = paris_today();
    let dates = match (requested_date, days) {
        (Some(date), _) => vec![
            NaiveDate::parse_from_str(date, DATE_FORMAT)
                .with_context(|| format!("Date invalide : {date}. Format attendu : YYYY-MM-DD"))?,
        ],
        (None, Some(days)) => next_days(today, days),
        (None, None) => cine_dates(today),
    };

    let cinema_ids = if let Some(cinema_id) = cinema_id {
        vec![cinema_id.to_owned()]
    } else {
        visible_cinema_ids(pool, departments).await?
    };
    if cinema_ids.is_empty() {
        bail!("Aucun cinéma visible à scraper pour ces départements : {departments:?}");
    }

    // Seul un run de référence alimente `/api/meta` (`last_scrape_at`) : la France entière,
    // ou le run de nuit (`--nightly`, qui peut se limiter à des départements). Un test sur
    // un cinéma ou un département ne doit pas faire croire que les données sont à jour.
    let kind = if cinema_id.is_none() && (departments.is_empty() || nightly) {
        "showtimes"
    } else {
        "showtimes_partial"
    };
    let started = Instant::now();
    let client = crate::client::build_client()?;
    let run_id = db::start_run(pool, kind).await?;

    let report = scrape_cinemas(pool, client, cinema_ids, &dates, min_interval).await;
    let purged = db::purge_past_showtimes(pool, &today.format(DATE_FORMAT).to_string()).await?;
    db::finish_run(pool, run_id, report.ok_dates, report.error_dates).await?;
    let summary = db::showtimes_summary(pool).await?;
    log_run_report(&report, &summary, purged, started.elapsed());

    // Code de sortie non nul pour le cron : coupe-circuit ou run entièrement raté.
    if report.circuit_breaker_tripped {
        bail!(
            "Coupe-circuit déclenché ({MAX_CONSECUTIVE_BLOCKS} refus 403/429 consécutifs malgré les pauses) : AlloCiné nous bloque peut-être"
        );
    }
    if report.ok_dates == 0 {
        bail!("Aucun couple cinéma/date n'a pu être scrapé");
    }
    Ok(())
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
    fn next_days_counts_today() {
        let friday = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        let days = next_days(friday, 7);
        assert_eq!(days.len(), 7);
        assert_eq!(days[0], friday);
        assert_eq!(days[6], NaiveDate::from_ymd_opt(2026, 10, 15).unwrap());
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
    fn run_report_counts_cinema_dates() {
        let mut report = RunReport::default();
        let complete = CinemaReport {
            completed_dates: 3,
            showtimes: 10,
            skipped_dates: 2,
        };
        let partial = CinemaReport {
            completed_dates: 1,
            showtimes: 2,
            skipped_dates: 0,
        };
        report.add_cinema(&complete, 3);
        report.add_cinema(&partial, 3);
        assert_eq!(
            (
                report.successful_cinemas,
                report.failed_cinemas,
                report.ok_dates,
                report.error_dates,
                report.showtimes,
                report.skipped_dates
            ),
            (1, 1, 4, 2, 12, 2)
        );
    }

    #[test]
    fn circuit_breaker_trips_on_three_consecutive_blocks() {
        let breaker = CircuitBreaker::default();
        breaker.observe_blocks(2);
        assert!(!breaker.is_tripped());
        breaker.observe_blocks(3);
        assert!(breaker.is_tripped());
        assert!(breaker.ensure_active().is_err());
    }

    #[test]
    fn pace_spaces_requests_by_the_interval() {
        let t0 = Instant::now();
        let mut pace = Pace::new(t0, MIN_INTERVAL);
        assert_eq!(pace.reserve(t0), t0);
        assert_eq!(pace.reserve(t0), t0 + MIN_INTERVAL);
        assert_eq!(pace.reserve(t0), t0 + MIN_INTERVAL * 2);
        // Après un long silence, pas de rafale pour « rattraper » : on part de maintenant.
        let later = t0 + Duration::from_secs(60);
        assert_eq!(pace.reserve(later), later);
    }

    #[test]
    fn block_pauses_and_halves_the_rate_once_per_pause() {
        let t0 = Instant::now();
        let mut pace = Pace::new(t0, MIN_INTERVAL);
        pace.reserve(t0);
        assert_eq!(pace.on_block(t0), Some(1));
        assert_eq!(pace.interval, MIN_INTERVAL * 2);
        assert!(pace.is_paused(t0 + BLOCK_PAUSE - Duration::from_millis(1)));
        assert_eq!(pace.reserve(t0), t0 + BLOCK_PAUSE);
        // Réponses des requêtes parties avant la pause : déjà comptées.
        assert_eq!(pace.on_block(t0 + Duration::from_secs(1)), None);
        assert_eq!(pace.interval, MIN_INTERVAL * 2);
        assert_eq!(pace.slowdowns, 1);
        // Après la pause, un nouveau refus ralentit encore, avec la même pause.
        let t1 = t0 + BLOCK_PAUSE;
        assert_eq!(pace.on_block(t1), Some(2));
        assert_eq!(pace.interval, MIN_INTERVAL * 4);
        assert_eq!(pace.reserve(t1), t1 + BLOCK_PAUSE);
        assert_eq!(pace.slowdowns, 2);
    }

    #[test]
    fn pauses_cover_a_several_minute_block_before_the_circuit_breaker() {
        let start = Instant::now();
        let mut now = start;
        let mut pace = Pace::new(now, MIN_INTERVAL);
        for _ in 1..MAX_CONSECUTIVE_BLOCKS {
            pace.on_block(now);
            now = pace.paused_until;
        }
        // 2 pauses de 5 min avant le 3e refus (et le coupe-circuit) : plus que les 7 min 30
        // de blocage vues au pire.
        assert_eq!(now - start, Duration::from_secs(600));
        for _ in 0..10 {
            pace.on_block(now);
            now = pace.paused_until;
        }
        assert_eq!(pace.interval, MAX_INTERVAL);
    }

    #[test]
    fn a_slower_starting_pace_is_also_the_floor() {
        let t0 = Instant::now();
        let second = Duration::from_secs(1);
        let mut pace = Pace::new(t0, second);
        assert_eq!(pace.reserve(t0), t0);
        assert_eq!(pace.reserve(t0), t0 + second);
        pace.on_block(t0);
        assert_eq!(pace.interval, second * 2);
        for _ in 0..SPEEDUP_AFTER * 50 {
            pace.on_success(pace.paused_until);
        }
        assert_eq!(pace.interval, second);
    }

    #[test]
    fn a_starting_pace_slower_than_the_maximum_is_kept_after_blocks() {
        let t0 = Instant::now();
        let slow = MAX_INTERVAL * 2;
        let mut pace = Pace::new(t0, slow);
        pace.on_block(t0);
        assert_eq!(pace.interval, slow);
    }

    #[test]
    fn success_during_a_pause_does_not_reset_the_block_count() {
        let t0 = Instant::now();
        let mut pace = Pace::new(t0, MIN_INTERVAL);
        pace.on_block(t0);
        pace.on_success(t0 + Duration::from_secs(1));
        assert_eq!(pace.on_block(t0 + BLOCK_PAUSE), Some(2));
        // Hors pause, une réponse correcte remet le compteur à zéro.
        let after = pace.paused_until;
        pace.on_success(after);
        assert_eq!(pace.on_block(after), Some(1));
    }

    #[test]
    fn pace_speeds_up_gradually_after_successes() {
        let t0 = Instant::now();
        let mut pace = Pace::new(t0, MIN_INTERVAL);
        pace.on_block(t0);
        let slow = pace.interval;
        let now = pace.paused_until;
        for _ in 0..SPEEDUP_AFTER - 1 {
            pace.on_success(now);
        }
        assert_eq!(pace.interval, slow);
        pace.on_success(now);
        assert_eq!(pace.interval, slow * 4 / 5);
        // Jamais plus vite que le rythme de départ.
        for _ in 0..SPEEDUP_AFTER * 50 {
            pace.on_success(now);
        }
        assert_eq!(pace.interval, MIN_INTERVAL);
    }

    #[test]
    fn a_block_resets_the_success_streak() {
        let t0 = Instant::now();
        let mut pace = Pace::new(t0, MIN_INTERVAL);
        pace.on_block(t0);
        let now = pace.paused_until;
        for _ in 0..SPEEDUP_AFTER - 1 {
            pace.on_success(now);
        }
        pace.on_block(now);
        let slow = pace.interval;
        pace.on_success(pace.paused_until);
        assert_eq!(pace.interval, slow);
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
            "INSERT INTO cinemas (id, name, name_search, lat, lng, department, updated_at) VALUES
             ('PARIS', 'Paris', 'paris', 48.0, 2.0, '75', datetime('now')),
             ('LYON', 'Lyon', 'lyon', 45.0, 4.0, '69', datetime('now')),
             ('NO_GEO', 'Sans GPS', 'sans gps', NULL, NULL, '75', datetime('now')),
             ('NO_LNG', 'Sans longitude', 'sans longitude', 48.0, NULL, '75', datetime('now')),
             ('OLD', 'Ancien', 'ancien', 48.0, 2.0, '75', '2000-01-01 00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            visible_cinema_ids(&pool, &["75".to_owned()]).await.unwrap(),
            ["PARIS"]
        );
        assert_eq!(
            visible_cinema_ids(&pool, &["69".to_owned(), "75".to_owned()])
                .await
                .unwrap(),
            ["LYON", "PARIS"]
        );
        assert!(
            visible_cinema_ids(&pool, &["13".to_owned()])
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            visible_cinema_ids(&pool, &[]).await.unwrap(),
            ["LYON", "PARIS"]
        );
    }
}
