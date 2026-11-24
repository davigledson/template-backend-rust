//! Módulos de negócio. Cada um é dono das suas rotas, regras e tabelas.

pub mod auth;
pub mod email;
pub mod users;

use utoipa_axum::router::OpenApiRouter;

use crate::core::state::AppState;

/// Junta os routers de todos os módulos, já com os paths documentados.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::router())
        .merge(users::router())
}
