//! Confirmação de e-mail: consumo do código e reenvio público.
//!
//! O reenvio é público por e-mail, como a recuperação de senha: sempre devolve
//! `204`, exista ou não a conta, para não permitir enumeração. O único `429` é
//! o rate limit por conta/IP, que qualquer um pode disparar.

use crate::{
    core::{
        auth::{
            rate_limit::{LimitDecision, RateLimiter},
            verification_token,
        },
        error::AppError,
        metrics,
        state::AppState,
    },
    modules::{
        auth::dto::{ResendVerificationRequest, VerifyEmailRequest},
        users::service as users_service,
    },
};

/// Consome o código e marca o e-mail como confirmado.
///
/// Repetir com o mesmo código devolve 400: o código é de uso único. Se a conta
/// já estava confirmada, o resultado é 204 (idempotente do ponto de vista do
/// usuário). O código só vale se o e-mail e o `token_version` não mudaram.
#[tracing::instrument(skip(state, request))]
pub async fn verify(state: &AppState, request: VerifyEmailRequest) -> Result<(), AppError> {
    let Some((user_id, token_version, email_fingerprint)) =
        verification_token::consume(&state.cache, &request.token).await?
    else {
        return Err(AppError::BadRequest(
            "código inválido ou expirado".to_owned(),
        ));
    };

    if users_service::mark_email_verified(&state.db, user_id, token_version, &email_fingerprint)
        .await?
    {
        metrics::email_verified();
    }

    Ok(())
}

/// Reenvia o código de confirmação para o e-mail informado.
///
/// Sempre devolve `204`; contas inexistentes, inativas ou já confirmadas não
/// recebem nada e o cliente não consegue distinguir os casos.
#[tracing::instrument(skip(state, request, client_ip))]
pub async fn resend(
    state: &AppState,
    request: ResendVerificationRequest,
    client_ip: &str,
) -> Result<(), AppError> {
    let email = users_service::normalize_email(&request.email);
    if email.is_empty() {
        return Ok(());
    }

    let limiter = RateLimiter::new(&state.cache, state.config.rate_limit.window());
    let account_key = RateLimiter::verification_account_key(&email);
    let ip_key = RateLimiter::verification_ip_key(client_ip);

    enforce_limit(
        &limiter,
        &account_key,
        state.config.rate_limit.verification_account_attempts,
        "account",
    )
    .await?;
    enforce_limit(
        &limiter,
        &ip_key,
        state.config.rate_limit.verification_ip_attempts,
        "ip",
    )
    .await?;
    register_attempt(&limiter, &account_key).await;
    register_attempt(&limiter, &ip_key).await;

    let Some(user) = users_service::find_by_email(&state.db, &email).await? else {
        return Ok(());
    };
    if !user.active || user.email_verified_at.is_some() {
        return Ok(());
    }

    users_service::send_email_verification(&state.db, &state.cache, &user).await;
    Ok(())
}

async fn enforce_limit(
    limiter: &RateLimiter<'_>,
    key: &str,
    max_attempts: u32,
    scope: &'static str,
) -> Result<(), AppError> {
    match limiter.check(key, max_attempts).await {
        Ok(LimitDecision::Allowed) => Ok(()),
        Ok(LimitDecision::Blocked { retry_after_secs }) => {
            tracing::warn!(scope, retry_after_secs, "reenvio de confirmação bloqueado");
            Err(AppError::TooManyRequests { retry_after_secs })
        }
        Err(error) => {
            tracing::warn!(error = %error, scope, "rate limit indisponível; seguindo sem limite");
            Ok(())
        }
    }
}

async fn register_attempt(limiter: &RateLimiter<'_>, key: &str) {
    if let Err(error) = limiter.register_failure(key).await {
        tracing::warn!(error = %error, "falha ao registrar reenvio de confirmação");
    }
}
