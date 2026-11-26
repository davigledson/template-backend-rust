//! Montagem da aplicação: estado + rotas + camadas HTTP.

use axum::{Router, http::HeaderMap, middleware, routing::get};

use crate::{
    core::{client_ip, http::apply_layers, idempotency, metrics, state::AppState},
    routes,
};

pub fn build(state: AppState) -> Router {
    let config = state.config.clone();
    let idempotency_state = state.clone();
    let (metrics_layer, metrics_handle) = metrics::pair();

    let mut router = routes::router(config.docs.enabled)
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            idempotency_state,
            idempotency::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            config.clone(),
            client_ip::resolve,
        ))
        .layer(metrics_layer);

    if config.metrics.enabled {
        let token = config.metrics.token().map(str::to_owned);
        router = router.route(
            "/metrics",
            get(move |headers: HeaderMap| {
                let handle = metrics_handle.clone();
                let token = token.clone();
                async move { metrics::render(handle, token.as_deref(), &headers) }
            }),
        );
    }

    apply_layers(router, config)
}
