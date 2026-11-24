//! Handlers de gestão de usuários (somente administradores).
//!
//! Todas as rotas exigem `AuthUser` e chamam `require_session` para conferir
//! `active`, `token_version` e papel no banco. Os handlers ficam divididos por
//! sub-recurso: coleção (`list`, `create`) e item (`get`, `update`, `delete`,
//! `password`).

mod collection;
mod item;
mod password;

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    core::{
        auth::{extractor::AuthUser, permission::Permission},
        error::AppError,
        state::AppState,
    },
    modules::users::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(collection::list, collection::create))
        .routes(routes!(item::get, item::update, item::delete))
        .routes(routes!(password::change))
}

/// Checa a permissão e, depois, se o token ainda vale no banco.
async fn authorize(
    state: &AppState,
    auth: &AuthUser,
    permission: Permission,
) -> Result<(), AppError> {
    auth.require(permission)?;
    service::require_session(&state.db, auth).await?;
    Ok(())
}
