//! Sessão: refresh com rotação e detecção de reuso, e logout.

use crate::{
    core::{auth::role::Role, error::AppError, metrics, state::AppState},
    modules::{
        auth::{
            repository::{self, ConsumeOutcome},
            service::tokens::{IssuedTokens, issue_tokens},
        },
        users::service as users_service,
    },
};

#[tracing::instrument(skip(state, token))]
pub async fn refresh(state: &AppState, token: &str) -> Result<IssuedTokens, AppError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(invalid_refresh());
    }

    let ttl = state.config.jwt.refresh_ttl();
    let ticket = match repository::consume(&state.cache, token, ttl).await? {
        ConsumeOutcome::Active(ticket) => ticket,
        ConsumeOutcome::Reused(family_id) => {
            metrics::refresh_reuse();
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

    repository::touch_session(&state.cache, user.id, ticket.family_id, ttl).await?;

    issue_tokens(state, &user, role, ticket.family_id).await
}

#[tracing::instrument(skip(state, token))]
pub async fn logout(state: &AppState, token: Option<&str>) -> Result<(), AppError> {
    let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) else {
        return Ok(());
    };

    // Lê a família antes de revogar para apagar também os metadados.
    let ticket = repository::peek(&state.cache, token).await?;
    repository::revoke(&state.cache, token).await?;

    if let Some(ticket) = ticket {
        repository::remove_session(&state.cache, ticket.user_id, ticket.family_id).await?;
        metrics::session_revoked();
    }
    Ok(())
}

fn invalid_refresh() -> AppError {
    AppError::Unauthorized("refresh token inválido ou expirado".to_owned())
}
