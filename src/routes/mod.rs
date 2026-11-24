//! Junta os routers. Nenhuma regra de negócio aqui.

mod openapi;

use axum::Router;

use crate::{
    core::{error::AppError, health, state::AppState},
    routes::openapi as docs,
};

pub fn router() -> Router<AppState> {
    let (api, spec) = docs::build();

    Router::new()
        .merge(health::router())
        .merge(api)
        .merge(docs::router(spec))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

async fn not_found() -> AppError {
    AppError::NotFound("rota não encontrada".to_owned())
}

async fn method_not_allowed() -> AppError {
    AppError::MethodNotAllowed("método HTTP não suportado nesta rota".to_owned())
}
