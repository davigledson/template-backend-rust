//! Montagem da aplicação: estado + rotas + camadas HTTP.

use axum::Router;

use crate::{
    core::{http::apply_layers, state::AppState},
    routes,
};

pub fn build(state: AppState) -> Router {
    let config = state.config.clone();
    let router = routes::router().with_state(state);
    apply_layers(router, &config)
}
