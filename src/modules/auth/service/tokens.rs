//! Emissão do par de tokens: access JWT + refresh opaco gravado no Redis.
//!
//! O registro da sessão (IP/User-Agent) fica com o login: o refresh só
//! atualiza o último uso, preservando a data de criação.

use uuid::Uuid;

use crate::{
    core::{auth::role::Role, error::AppError, state::AppState},
    database::entity::users,
    modules::auth::{dto::TokenResponse, repository},
};

pub(super) async fn issue_tokens(
    state: &AppState,
    user: &users::Model,
    role: Role,
    family_id: Uuid,
) -> Result<TokenResponse, AppError> {
    let access_token =
        state
            .jwt
            .issue_access(user.id, &user.email, role, user.token_version, family_id)?;

    let ttl = state.config.jwt.refresh_ttl();
    let refresh_token = repository::generate_token();
    repository::store(
        &state.cache,
        &refresh_token,
        user.id,
        user.token_version,
        family_id,
        ttl,
    )
    .await?;

    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: state.jwt.access_ttl_secs(),
    })
}
