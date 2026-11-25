//! Módulos de negócio. Cada um é dono das suas rotas, regras e tabelas.

pub mod audit;
pub mod auth;
pub mod email;
pub mod users;

use utoipa_axum::router::OpenApiRouter;

use crate::core::state::AppState;

/// Junta os routers de todos os módulos, já com os paths documentados.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(audit::router())
        .merge(auth::router())
        .merge(users::router())
}
