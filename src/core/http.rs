//! Middleware HTTP: request-id, trace, CORS, compressão, timeout e limite de body.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{
        HeaderName, HeaderValue, Method, StatusCode,
        header::{
            CACHE_CONTROL, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
            X_FRAME_OPTIONS,
        },
    },
    middleware::{self, Next},
    response::Response,
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
pub fn apply_layers<S>(router: Router<S>, settings: Arc<Settings>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let security_settings = Arc::clone(&settings);

    router
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(cors_layer(&settings))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            settings.server.request_timeout(),
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(DefaultBodyLimit::max(settings.server.body_limit_bytes()))
        .layer(problemdetails::axum::PanicHandlerBuilder::new().build())
        .layer(middleware::from_fn_with_state(
            security_settings,
            security_headers,
        ))
}

/// Cabeçalhos de segurança e `no-store` para a API.
///
/// HSTS só é enviado quando `security.hsts_max_age_secs > 0`; o navegador
/// ignora o cabeçalho em HTTP, então ele depende de TLS no proxy/servidor.
async fn security_headers(
    State(settings): State<Arc<Settings>>,
    request: Request,
    next: Next,
) -> Response {
    let no_store =
        request.uri().path().starts_with("/api/v1") || request.uri().path() == "/metrics";

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );

    if settings.security.hsts_max_age_secs > 0
        && let Ok(value) =
            HeaderValue::from_str(&format!("max-age={}", settings.security.hsts_max_age_secs))
    {
        headers.insert(STRICT_TRANSPORT_SECURITY, value);
    }

    if no_store {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }

    response
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
