use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    http::{HeaderValue, Response, header},
    routing::get,
};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};
use tracing::info;

use crate::cinemas::departments::get_departments;

mod cinemas;
mod error;
mod geo;
mod meta;
mod movies;
mod params;
mod search;
mod types;

pub use error::{ApiResult, AppError};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Code INSEE du département → nom ("75" → "Paris"). Dans un `Arc` pour
    /// que le clone de l'état à chaque requête ne copie jamais la table.
    pub departments: Arc<HashMap<String, String>>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let departments = get_departments().map(|d| (d.code_insee, d.nom)).collect();
        AppState {
            pool,
            departments: Arc::new(departments),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/meta", get(meta::meta))
        .route("/api/cinemas", get(cinemas::list))
        .route("/api/cinemas/{id}", get(cinemas::get_one))
        .route("/api/cinemas/{id}/showtimes", get(cinemas::showtimes))
        .route("/api/movies", get(movies::list))
        .route("/api/movies/{id}", get(movies::get_one))
        .route("/api/movies/{id}/showtimes", get(movies::showtimes))
        .route("/api/search", get(search::search))
        .fallback(not_found)
        .with_state(state)
        // Les erreurs posent déjà `no-store` (AppError) : on ne remplit que les autres.
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            cache_control,
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// 2xx : 5 minutes en cache (service worker de la PWA) ; le reste : jamais.
fn cache_control<B>(response: &Response<B>) -> Option<HeaderValue> {
    Some(HeaderValue::from_static(
        if response.status().is_success() {
            "public, max-age=300"
        } else {
            "no-store"
        },
    ))
}

async fn not_found() -> AppError {
    AppError::NotFound("Route introuvable")
}

pub async fn serve(pool: SqlitePool, addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "API démarrée");
    axum::serve(listener, router(AppState::new(pool)))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("API arrêtée");
    Ok(())
}

/// Ctrl-C en local, SIGTERM sous systemd (`systemctl stop` / `restart`) :
/// les requêtes en cours se terminent avant la sortie.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            // Sans SIGTERM, Ctrl-C reste disponible : on attend indéfiniment ici.
            Err(e) => {
                tracing::warn!("SIGTERM non écouté : {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("Arrêt demandé, fin des requêtes en cours");
}
