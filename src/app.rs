//! Montagem da aplicação: estado + rotas + camadas HTTP.

use axum::{Router, http::header::CONTENT_TYPE, middleware, routing::get};

use crate::{
    core::{client_ip, http::apply_layers, idempotency, metrics, state::AppState},
    routes,
};

pub fn build(state: AppState) -> Router {
    let config = state.config.clone();
    let idempotency_state = state.clone();
    let (metrics_layer, metrics_handle) = metrics::pair();

    let router = routes::router()
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            idempotency_state,
            idempotency::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            config.clone(),
            client_ip::resolve,
        ))
        .layer(metrics_layer)
        .route(
            "/metrics",
            get(move || {
                let handle = metrics_handle.clone();
                async move {
                    (
                        [(CONTENT_TYPE, "text/plain; version=0.0.4")],
                        handle.render(),
                    )
                }
            }),
        );

    apply_layers(router, &config)
}
