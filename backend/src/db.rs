use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::{env, time::Duration};

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!().run(pool).await?;
    Ok(())
}

pub async fn establish_connection() -> anyhow::Result<SqlitePool> {
    let database_url =
        env::var("DATABASE_URL").context("Variable d'environnement DATABASE_URL introuvable")?;
    let options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(5000))
        .foreign_keys(true);

    // Petit pool : en WAL, les lectures (l'essentiel du travail de l'API)
    // se font en parallèle, mais SQLite n'a qu'un écrivain à la fois, donc
    // inutile d'ouvrir beaucoup de connexions. Les conflits d'écriture entre
    // processus (serve / scrape) sont absorbés par le busy_timeout.
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;

    Ok(pool)
}
