pub mod allocine;
mod db;
mod mapping;

use anyhow::{Context, bail};
use chrono::{Datelike, Days, NaiveDate, Utc};
use chrono_tz::Europe::Paris;
use sqlx::SqlitePool;
use tracing::info;

fn cine_dates(today: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = vec![today, today + Days::new(1), today + Days::new(2)];
    if today.weekday() == chrono::Weekday::Wed {
        dates.push(today + Days::new(6));
    }
    dates
}

pub async fn scrape(
    pool: &SqlitePool,
    cinema_id: Option<&str>,
    requested_date: Option<&str>,
) -> anyhow::Result<()> {
    let Some(cinema_id) = cinema_id else {
        bail!(
            "Le scraping global sera disponible au lot E ; indiquez --cinema pour cibler une salle"
        );
    };

    let dates = if let Some(date) = requested_date {
        vec![
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|| format!("Date invalide : {date}. Format attendu : YYYY-MM-DD"))?,
        ]
    } else {
        let today = Utc::now().with_timezone(&Paris).date_naive();
        cine_dates(today)
    };

    let client = crate::client::build_client()?;
    for date in dates {
        let date = date.format("%Y-%m-%d").to_string();
        let movies = allocine::fetch_showtimes(&client, cinema_id, &date)
            .await
            .with_context(|| format!("Scraping de {cinema_id} le {date}"))?;
        let stats = ScrapeStats {
            movies: movies.len(),
            showtimes: allocine::count_showtimes(&movies),
        };
        db::save_showtimes(pool, cinema_id, &date, &movies).await?;
        info!(
            cinema = cinema_id,
            %date,
            films = stats.movies,
            seances = stats.showtimes,
            "Scraping enregistré"
        );
    }
    Ok(())
}

#[derive(Debug, Default)]
struct ScrapeStats {
    movies: usize,
    showtimes: usize,
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
}
