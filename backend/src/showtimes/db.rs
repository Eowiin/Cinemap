use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::{Sqlite, Transaction};

use crate::text::normalize;

use super::{
    allocine::MovieResult,
    mapping::{booking_url, formats, full_name, runtime_minutes, version},
};

pub(super) async fn save_showtimes(
    pool: &SqlitePool,
    cinema_id: &str,
    date: &str,
    movies: &[MovieResult],
) -> Result<()> {
    let mut tx = pool.begin().await?;

    for entry in movies {
        upsert_movie(&mut tx, &entry.movie).await?;
    }

    sqlx::query("DELETE FROM showtimes WHERE cinema_id = ? AND date = ?")
        .bind(cinema_id)
        .bind(date)
        .execute(&mut *tx)
        .await?;

    for entry in movies {
        for showtime in entry.showtimes.values().flatten() {
            insert_showtime(&mut tx, cinema_id, date, entry, showtime).await?;
        }
    }

    tx.commit().await?;
    Ok(())
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
) -> Result<()> {
    let Some(mapped_version) = version(showtime, &entry.movie.languages) else {
        tracing::warn!(
            cinema_id,
            date,
            showtime_id = showtime.internal_id,
            diffusion_version = %showtime.diffusion_version,
            "Séance ignorée : version AlloCiné inconnue"
        );
        return Ok(());
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::showtimes::allocine::Response;

    const C0159_PAGE_1: &str =
        include_str!("../../tests/fixtures/showtimes-C0159-2026-10-06-p1.json");

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
