//! Sessão: refresh com rotação e detecção de reuso, e logout.

use crate::{
    core::{auth::role::Role, error::AppError, state::AppState},
    modules::{
        auth::{
            dto::{RefreshRequest, TokenResponse},
            repository::{self, ConsumeOutcome},
            service::tokens::issue_tokens,
        },
        users::service as users_service,
    },
};

pub async fn refresh(state: &AppState, request: RefreshRequest) -> Result<TokenResponse, AppError> {
    let token = request.refresh_token.trim();
    if token.is_empty() {
        return Err(invalid_refresh());
    }

    let ttl = state.config.jwt.refresh_ttl();
    let ticket = match repository::consume(&state.cache, token, ttl).await? {
        ConsumeOutcome::Active(ticket) => ticket,
        ConsumeOutcome::Reused(family_id) => {
            repository::revoke_family(&state.cache, family_id, ttl).await?;
            tracing::warn!(
                family = %family_id,
                "reuso de refresh token detectado; família revogada"
            );
            return Err(AppError::Unauthorized(
                "reuso de refresh token detectado; faça login novamente".to_owned(),
            ));
        }
        ConsumeOutcome::Unknown => return Err(invalid_refresh()),
    };

    if repository::is_family_revoked(&state.cache, ticket.family_id).await? {
        return Err(invalid_refresh());
    }

    let user = users_service::find_by_id(&state.db, ticket.user_id)
        .await?
        .filter(|user| user.active)
        .ok_or_else(invalid_refresh)?;

    if user.token_version != ticket.token_version {
        return Err(invalid_refresh());
    }

    let role = Role::parse(&user.role)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {}", user.role)))?;

    issue_tokens(state, &user, role, ticket.family_id).await
}

pub async fn logout(state: &AppState, request: RefreshRequest) -> Result<(), AppError> {
    let token = request.refresh_token.trim();
    if !token.is_empty() {
        repository::revoke(&state.cache, token).await?;
    }
    Ok(())
}

fn invalid_refresh() -> AppError {
    AppError::Unauthorized("refresh token inválido ou expirado".to_owned())
}
