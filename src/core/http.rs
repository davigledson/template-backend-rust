//! Middleware HTTP: request-id, trace, CORS, compressão, timeout e limite de body.

use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, StatusCode},
};
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::config::settings::Settings;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Aplica as camadas transversais. A ordem importa: a última camada é a mais externa.
pub fn apply_layers<S>(router: Router<S>, settings: &Settings) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(cors_layer(settings))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            settings.server.request_timeout(),
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(DefaultBodyLimit::max(settings.server.body_limit_bytes()))
        .layer(problemdetails::axum::PanicHandlerBuilder::new().build())
}

fn cors_layer(settings: &Settings) -> CorsLayer {
    let methods: Vec<Method> = settings
        .cors
        .allowed_methods
        .iter()
        .filter_map(|method| method.parse().ok())
        .collect();

    let headers: Vec<HeaderName> = settings
        .cors
        .allowed_headers
        .iter()
        .filter_map(|header| header.parse().ok())
        .collect();

    let layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(headers)
        .max_age(Duration::from_secs(settings.cors.max_age_secs));

    if settings
        .cors
        .allowed_origins
        .iter()
        .any(|origin| origin == "*")
    {
        return layer.allow_origin(Any);
    }

    let origins: Vec<HeaderValue> = settings
        .cors
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    layer.allow_origin(AllowOrigin::list(origins))
}
