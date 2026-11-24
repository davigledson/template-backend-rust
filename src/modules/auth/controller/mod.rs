//! Handlers de autenticação.
//!
//! Dividido entre sessão (`login`, `refresh`, `logout`) e perfil do próprio
//! usuário (`me`, `update_me`, `change_my_password`).

mod profile;
mod session;

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::core::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(session::login))
        .routes(routes!(session::refresh))
        .routes(routes!(session::logout))
        .routes(routes!(profile::me))
        .routes(routes!(profile::update_me))
        .routes(routes!(profile::change_my_password))
}
