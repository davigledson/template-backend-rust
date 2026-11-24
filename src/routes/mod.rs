//! Junta os routers. Nenhuma regra de negócio aqui.

use axum::Router;

use crate::core::{error::AppError, health, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
    // Fase 4+: .nest("/api/v1", modules::router())
}

async fn not_found() -> AppError {
    AppError::NotFound("rota não encontrada".to_owned())
}

async fn method_not_allowed() -> AppError {
    AppError::MethodNotAllowed("método HTTP não suportado nesta rota".to_owned())
}
