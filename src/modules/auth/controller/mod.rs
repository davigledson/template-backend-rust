//! Handlers de autenticação.
//!
//! Dividido entre sessão (`login`, `refresh`, `logout`), perfil do próprio
//! usuário (`me`, `update_me`, `change_my_password`), recuperação de senha,
//! confirmação de e-mail e sessões por dispositivo.

mod devices;
mod password;
mod profile;
mod session;
mod verification;

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::core::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(session::login))
        .routes(routes!(session::refresh))
        .routes(routes!(session::logout))
        .routes(routes!(password::forgot))
        .routes(routes!(password::reset))
        .routes(routes!(verification::verify))
        .routes(routes!(verification::resend))
        .routes(routes!(devices::list))
        .routes(routes!(devices::revoke))
        .routes(routes!(profile::me))
        .routes(routes!(profile::update_me))
        .routes(routes!(profile::change_my_password))
}
