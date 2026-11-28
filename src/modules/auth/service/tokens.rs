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

/// Par emitido internamente: o access vai no corpo, o refresh vai no cookie.
pub struct IssuedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

impl From<IssuedTokens> for TokenResponse {
    fn from(tokens: IssuedTokens) -> Self {
        Self {
            access_token: tokens.access_token,
            token_type: tokens.token_type,
            expires_in: tokens.expires_in,
        }
    }
}

pub(super) async fn issue_tokens(
    state: &AppState,
    user: &users::Model,
    role: Role,
    family_id: Uuid,
) -> Result<IssuedTokens, AppError> {
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

    Ok(IssuedTokens {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: state.jwt.access_ttl_secs(),
    })
}
