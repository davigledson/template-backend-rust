//! Documento OpenAPI e UI Scalar.
//!
//! O documento é gerado a partir dos mesmos routers que atendem a API.

use std::sync::Arc;

use axum::{Json, Router, response::Html, routing::get};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::Scalar;

use crate::{core::state::AppState, modules};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Template Backend Rust",
        version = env!("CARGO_PKG_VERSION"),
        description = "API de referência com autenticação JWT, refresh rotativo e gestão de usuários.",
    ),
    tags(
        (name = "auth", description = "Login, refresh, logout e perfil"),
        (name = "users", description = "Gestão de usuários (somente admin)"),
    ),
    modifiers(&SecurityAddon),
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

/// Monta o router da API e o documento OpenAPI correspondente.
pub fn build() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", modules::router())
        .split_for_parts()
}

/// Rotas públicas da documentação: spec JSON e UI Scalar.
pub fn router(openapi: utoipa::openapi::OpenApi) -> Router<AppState> {
    let spec = Arc::new(openapi);
    let spec_json = Arc::clone(&spec);
    let ui = Arc::new(
        Scalar::new((*spec).clone())
            .title("Template Backend Rust — API")
            .to_html(),
    );

    Router::<AppState>::new()
        .route(
            "/api-docs/openapi.json",
            get(move || {
                let spec = Arc::clone(&spec_json);
                async move { Json((*spec).clone()) }
            }),
        )
        .route(
            "/docs",
            get(move || {
                let ui = Arc::clone(&ui);
                async move { Html((*ui).clone()) }
            }),
        )
        // Evita 404 de favicon no console do navegador.
        .route(
            "/favicon.ico",
            get(|| async { axum::http::StatusCode::NO_CONTENT }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documento_lista_rotas_publicas() {
        let (_, openapi) = build();

        for route in [
            "/api/v1/auth/login",
            "/api/v1/auth/refresh",
            "/api/v1/auth/logout",
            "/api/v1/auth/me",
            "/api/v1/users",
            "/api/v1/users/{id}",
            "/api/v1/users/{id}/password",
        ] {
            assert!(
                openapi.paths.paths.contains_key(route),
                "rota ausente: {route}"
            );
        }
    }

    #[test]
    fn documento_declara_bearer() {
        let (_, openapi) = build();
        let components = openapi.components.expect("components");
        assert!(components.security_schemes.contains_key("bearer"));
    }

    #[test]
    fn snapshot_do_contrato() {
        let (_, openapi) = build();
        let value = serde_json::to_value(openapi).expect("documento serializável");
        insta::assert_json_snapshot!("openapi", value);
    }
}
