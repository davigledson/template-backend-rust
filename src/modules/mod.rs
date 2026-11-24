//! Módulos de negócio. Cada um é dono das suas rotas, regras e tabelas.

pub mod auth;
pub mod users;

use axum::Router;

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().nest("/auth", auth::router())
}
