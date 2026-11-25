//! Confirmação de e-mail: consumo do código e reenvio.

use crate::{
    core::{
        auth::{
            extractor::AuthUser,
            rate_limit::{LimitDecision, RateLimiter},
            verification_token,
        },
        error::AppError,
        metrics,
        state::AppState,
    },
    modules::{auth::dto::VerifyEmailRequest, users::service as users_service},
};

/// Consome o código e marca o e-mail como confirmado.
///
/// Repetir com o mesmo código devolve 400: o código é de uso único. Se a conta
/// já estava confirmada, o resultado é 204 (idempotente do ponto de vista do
/// usuário).
#[tracing::instrument(skip(state, request))]
pub async fn verify(state: &AppState, request: VerifyEmailRequest) -> Result<(), AppError> {
    let Some(user_id) = verification_token::consume(&state.cache, &request.token).await? else {
        return Err(AppError::BadRequest(
            "código inválido ou expirado".to_owned(),
        ));
    };

    if users_service::mark_email_verified(&state.db, user_id).await? {
        metrics::email_verified();
    }

    Ok(())
}

/// Reenvia o código para o e-mail do usuário autenticado.
#[tracing::instrument(skip(state, auth))]
pub async fn resend(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    let user = users_service::require_session(&state.db, auth).await?;
    if user.email_verified_at.is_some() {
        return Ok(());
    }

    let limiter = RateLimiter::new(&state.cache, state.config.rate_limit.window());
    let key = RateLimiter::verification_key(user.id);

    match limiter
        .check(&key, state.config.rate_limit.verification_user_attempts)
        .await
    {
        Ok(LimitDecision::Allowed) => {}
        Ok(LimitDecision::Blocked { retry_after_secs }) => {
            return Err(AppError::TooManyRequests { retry_after_secs });
        }
        Err(error) => {
            tracing::warn!(error = %error, "rate limit indisponível; seguindo sem limite");
        }
    }

    if let Err(error) = limiter.register_failure(&key).await {
        tracing::warn!(error = %error, "falha ao registrar reenvio de confirmação");
    }

    users_service::send_email_verification(&state.db, &state.cache, &user).await;
    Ok(())
}
