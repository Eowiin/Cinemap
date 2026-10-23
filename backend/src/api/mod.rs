use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{
    Router,
    http::{HeaderValue, Response, header},
    routing::{get, post},
};
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};
use tracing::{info, warn};

use crate::cinemas::departments::get_departments;

mod cinemas;
mod cities;
mod error;
mod events;
mod geo;
mod meta;
mod movies;
mod pages;
mod params;
mod search;
mod showtimes;
pub mod types;

pub use error::{ApiResult, AppError};
pub use pages::PageTemplate;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Code INSEE du département → nom ("75" → "Paris"). Dans un `Arc` pour
    /// que le clone de l'état à chaque requête ne copie jamais la table.
    pub departments: Arc<HashMap<String, String>>,
    /// `index.html` du front, pour les pages HTML ; `None` en dev sans build du front.
    pub pages: Option<Arc<pages::PageTemplate>>,
    /// Index des villes, recalculé toutes les 10 minutes au plus.
    pub cities: Arc<cities::CityCache>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let departments = get_departments().map(|d| (d.code_insee, d.nom)).collect();
        AppState {
            pool,
            departments: Arc::new(departments),
            pages: None,
            cities: Arc::default(),
        }
    }

    pub fn with_pages(mut self, template: pages::PageTemplate) -> Self {
        self.pages = Some(Arc::new(template));
        self
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/meta", get(meta::meta))
        .route("/api/cinemas", get(cinemas::list))
        .route("/api/cinemas/{id}", get(cinemas::get_one))
        .route("/api/cinemas/{id}/showtimes", get(cinemas::showtimes))
        .route("/api/cities/{slug}", get(cities::get_one))
        .route("/api/events", post(events::record))
        .route("/api/movies", get(movies::list))
        .route("/api/movies/{id}", get(movies::get_one))
        .route("/api/movies/{id}/showtimes", get(movies::showtimes))
        .route("/api/search", get(search::search))
        .route("/api/showtimes", get(showtimes::list))
        // Pages HTML : nginx sert les fichiers du front et passe le reste ici.
        .route("/", get(pages::home))
        .route("/film/{segment}", get(pages::movie))
        .route("/cinema/{segment}", get(pages::cinema))
        .route("/ville/{slug}", get(pages::city))
        .route("/programme", get(pages::programme_page))
        .route("/a-propos", get(pages::about))
        .route("/sitemap.xml", get(pages::sitemap))
        .fallback(pages::fallback)
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

/// Dossier du front : `--www`, sinon `www/` à côté du binaire (une release sur le VPS :
/// `/proc/self/exe` suit le lien `current`), sinon le build local `../frontend/dist`.
fn www_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if explicit.is_some() {
        return explicit;
    }
    let beside_binary = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("www")));
    beside_binary
        .into_iter()
        .chain([PathBuf::from("../frontend/dist")])
        .find(|dir| dir.join("index.html").is_file())
}

pub async fn serve(pool: SqlitePool, addr: &str, www: Option<PathBuf>) -> anyhow::Result<()> {
    let mut state = AppState::new(pool);
    match www_dir(www) {
        Some(dir) => {
            // Un gabarit cassé arrête le démarrage : le health check du déploiement
            // garde alors la release précédente.
            let template = pages::PageTemplate::load(&dir.join("index.html"))?;
            info!(dir = %dir.display(), "Pages HTML servies depuis le front");
            state = state.with_pages(template);
        }
        None => warn!("index.html du front introuvable : pages HTML désactivées"),
    }
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "API démarrée");
    axum::serve(listener, router(state))
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
