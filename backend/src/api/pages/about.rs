use axum::{extract::State, http::StatusCode, response::Response};

use super::html::Head;
use super::page;
use crate::api::AppState;

/// Même fragment que la page de l'app (`AboutPage.svelte`) : un seul texte à tenir à jour,
/// inclus dans le binaire à la compilation.
const ABOUT_HTML: &str = include_str!("../../../../frontend/src/content/about.html");

/// `/a-propos` : mentions légales, vie privée, sources. Indexable, mais hors du sitemap
/// (rien à y chercher ; le lien en pied de chaque page, `FOOTER`, suffit aux robots).
pub async fn about(State(state): State<AppState>) -> Response {
    let head = Head {
        title: "À propos et mentions légales · Cinemap",
        description: "Mentions légales, vie privée (ni cookie ni compte) et sources des données \
                      de Cinemap : AlloCiné, TMDB, OpenStreetMap, CNC.",
        path: "/a-propos",
        image: None,
        og_type: "website",
        noindex: false,
        json_ld: None,
    };
    let body = format!("<div class=\"ssr\">{ABOUT_HTML}</div>");
    page(&state, StatusCode::OK, &head, &body)
}
