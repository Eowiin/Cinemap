use axum::{Router, routing::get};
use backend::cinemas::import_cinemas;
use backend::cli::{Cli, SubCommands};
use backend::db;
use clap::Parser;
use tracing_subscriber::EnvFilter;

async fn serve() -> anyhow::Result<()> {
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    Ok(axum::serve(listener, app).await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

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
        SubCommands::Scrape { cinema, date } => {
            backend::showtimes::scrape(&pool, cinema.as_deref(), date.as_deref()).await
        }
        SubCommands::Serve => serve().await,
    }
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
