use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::error;

/// Erreur d'un handler, rendue au format du contrat :
/// `{ "error": { "code": "not_found", "message": "Cinéma introuvable" } }`.
#[derive(Debug)]
pub enum AppError {
    /// Paramètre invalide : le message est construit avec la valeur reçue.
    BadRequest(String),
    /// Ressource absente : message fixe, aucune allocation.
    NotFound(&'static str),
    /// Erreur SQL ou autre : loggée en entier, jamais montrée au client.
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(message) => (StatusCode::BAD_REQUEST, "bad_request", message),
            AppError::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message.to_owned()),
            AppError::Internal(e) => {
                error!(error = format!("{e:#}"), "Erreur interne de l'API");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Erreur interne".to_owned(),
                )
            }
        };

        let body = json!({ "error": { "code": code, "message": message } });
        let mut response = (status, Json(body)).into_response();
        // Une erreur ne doit jamais rester en cache chez le visiteur.
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn render(error: AppError) -> (StatusCode, serde_json::Value, Option<HeaderValue>) {
        let response = error.into_response();
        let status = response.status();
        let cache = response.headers().get(header::CACHE_CONTROL).cloned();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap(), cache)
    }

    #[tokio::test]
    async fn bad_request_keeps_its_message() {
        let (status, body, cache) = render(AppError::BadRequest("Date invalide : x".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "bad_request");
        assert_eq!(body["error"]["message"], "Date invalide : x");
        assert_eq!(cache.unwrap(), "no-store");
    }

    #[tokio::test]
    async fn not_found_uses_contract_code() {
        let (status, body, _) = render(AppError::NotFound("Cinéma introuvable")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["message"], "Cinéma introuvable");
    }

    #[tokio::test]
    async fn internal_hides_the_cause() {
        let (status, body, _) =
            render(AppError::Internal(anyhow::anyhow!("no such table: secret"))).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["code"], "internal");
        assert_eq!(body["error"]["message"], "Erreur interne");
    }
}
