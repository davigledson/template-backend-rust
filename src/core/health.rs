//! Endpoints de saúde.
//!
//! - `GET /health` — liveness: responde sempre que o processo está de pé.
//! - `GET /health/ready` — readiness: exige Postgres e Redis acessíveis.

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::core::{error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(live))
        .route("/health/ready", get(ready))
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
}

async fn live() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn ready(State(state): State<AppState>) -> Result<Json<Health>, AppError> {
    state.db.ping().await.map_err(|error| {
        tracing::error!(error = %error, "readiness: banco de dados indisponível");
        AppError::Unavailable("banco de dados indisponível".to_owned())
    })?;

    state.cache.ping().await.map_err(|error| {
        tracing::error!(error = %error, "readiness: cache indisponível");
        AppError::Unavailable("cache indisponível".to_owned())
    })?;

    Ok(Json(Health { status: "ready" }))
}
