use backend::cinemas::import_cinemas;
use backend::cli::{Cli, SubCommands};
use backend::db;
use clap::Parser;
use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Pas de couleurs ANSI hors terminal (journald sous systemd).
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    let args = Cli::parse();
    let pool = db::establish_connection().await?;

    db::run_migrations(&pool).await?;

    match args.command {
        SubCommands::ImportCinemas => import_cinemas(&pool).await,
        SubCommands::Geocode { all } => {
            let client = backend::client::build_client()?;
            backend::cinemas::geocode::geocode_cinemas(&pool, &client, all).await
        }
        SubCommands::EnrichCnc => {
            let client = backend::client::build_client()?;
            backend::cinemas::cnc::enrich_cinemas(&pool, &client).await
        }
        SubCommands::Scrape {
            cinema,
            date,
            department,
        } => {
            backend::showtimes::scrape(
                &pool,
                cinema.as_deref(),
                date.as_deref(),
                department.as_deref(),
            )
            .await
        }
        SubCommands::Tmdb { all } => backend::tmdb::sync(&pool, all).await,
        SubCommands::Serve { addr } => backend::api::serve(pool, &addr).await,
    }
}
