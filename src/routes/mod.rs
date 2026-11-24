//! Junta os routers. Nenhuma regra de negócio aqui.

use axum::Router;

use crate::{
    core::{error::AppError, health, state::AppState},
    modules,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/api/v1", modules::router())
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

async fn not_found() -> AppError {
    AppError::NotFound("rota não encontrada".to_owned())
}

async fn method_not_allowed() -> AppError {
    AppError::MethodNotAllowed("método HTTP não suportado nesta rota".to_owned())
}
