use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
};
use serde::Deserialize;

use super::AppState;
use super::error::{ApiResult, AppError};
use crate::stats::{self, Event, Target};

/// `?e=book&t=C0013`, envoyé par `navigator.sendBeacon` : en query plutôt qu'en corps,
/// pour éviter les subtilités de type de contenu des beacons.
#[derive(Debug, Deserialize)]
pub struct EventQuery {
    e: Option<String>,
    t: Option<String>,
}

/// Robots qui exécutent le JavaScript (rendu Google, Lighthouse…) : ignorés sans erreur.
fn is_bot(headers: &HeaderMap) -> bool {
    let Some(agent) = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
    else {
        return true;
    };
    let agent = agent.to_ascii_lowercase();
    [
        "bot",
        "crawl",
        "spider",
        "headless",
        "lighthouse",
        "preview",
    ]
    .iter()
    .any(|word| agent.contains(word))
}

/// Compte un événement du front. 204 aussi pour un robot : il n'y a rien à lui dire.
pub async fn record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(raw): Query<EventQuery>,
) -> ApiResult<StatusCode> {
    let name = raw.e.as_deref().unwrap_or_default();
    let event = Event::parse(name)
        .ok_or_else(|| AppError::BadRequest(format!("Événement inconnu : {name}")))?;
    if is_bot(&headers) {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Seules les cibles qui existent sont comptées : impossible de remplir la table
    // de lignes inventées.
    let target = raw.t.unwrap_or_default();
    let known = match event.target() {
        Target::None if target.is_empty() => true,
        Target::None => {
            return Err(AppError::BadRequest(format!(
                "{name} n'a pas de cible : {target}"
            )));
        }
        Target::Cinema => {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM cinemas WHERE id = ?1) AS "found!: bool""#,
                target,
            )
            .fetch_one(&state.pool)
            .await?
        }
        Target::Movie => match target.parse::<i64>() {
            Ok(id) => {
                sqlx::query_scalar!(
                    r#"SELECT EXISTS (SELECT 1 FROM movies WHERE id = ?1) AS "found!: bool""#,
                    id,
                )
                .fetch_one(&state.pool)
                .await?
            }
            Err(_) => false,
        },
        Target::City => state
            .cities
            .get(&state.pool, &state.departments)
            .await?
            .get(&target)
            .is_some(),
    };
    if !known {
        return Err(AppError::NotFound("Cible inconnue"));
    }

    stats::record(&state.pool, event, &target).await?;
    Ok(StatusCode::NO_CONTENT)
}
