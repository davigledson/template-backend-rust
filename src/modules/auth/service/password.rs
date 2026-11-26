//! Recuperação de senha por código enviado ao e-mail.
//!
//! - pedido **sempre** devolve 204, exista ou não a conta: não há enumeração
//!   de e-mails (o único 429 é o rate limit, que qualquer um dispara);
//! - o código é opaco, de uso único, guardado como hash no Redis e morre se a
//!   senha/papel (`token_version`) ou o e-mail mudarem.

use crate::{
    core::{
        auth::{
            password,
            rate_limit::{LimitDecision, RateLimiter},
        },
        error::AppError,
        mail::mask_email,
        metrics,
        state::AppState,
    },
    modules::{
        auth::{
            dto::{ForgotPasswordRequest, ResetPasswordRequest},
            repository,
        },
        email::{service as email_service, templates as email_templates},
        users::service as users_service,
    },
};

#[tracing::instrument(skip(state, request, client_ip))]
pub async fn request_reset(
    state: &AppState,
    request: ForgotPasswordRequest,
    client_ip: &str,
) -> Result<(), AppError> {
    let email = users_service::normalize_email(&request.email);
    if email.is_empty() {
        return Ok(());
    }

    let limiter = RateLimiter::new(&state.cache, state.config.rate_limit.window());
    let account_key = RateLimiter::reset_account_key(&email);
    let ip_key = RateLimiter::reset_ip_key(client_ip);

    enforce_limit(
        &limiter,
        &account_key,
        state.config.rate_limit.password_reset_account_attempts,
        "account",
    )
    .await?;
    enforce_limit(
        &limiter,
        &ip_key,
        state.config.rate_limit.password_reset_ip_attempts,
        "ip",
    )
    .await?;
    register_attempt(&limiter, &account_key).await;
    register_attempt(&limiter, &ip_key).await;

    let Some(user) = users_service::find_by_email(&state.db, &email).await? else {
        return Ok(());
    };
    if !user.active {
        return Ok(());
    }

    let token =
        repository::issue_reset_token(&state.cache, user.id, user.token_version, &user.email)
            .await?;
    metrics::password_reset_requested();
    let message = email_templates::password_reset(&user.name, &user.email, &token);
    let dedupe_key = format!("password-reset:{}:{}", user.id, &token[..8]);

    if let Err(error) = email_service::enqueue(&state.db, message, dedupe_key).await {
        // O usuário não pode saber se a conta existe; o erro fica no log.
        tracing::warn!(
            error = %error,
            to = %mask_email(&user.email),
            "falha ao enfileirar o e-mail de recuperação"
        );
    }

    Ok(())
}

#[tracing::instrument(skip(state, request))]
pub async fn confirm_reset(
    state: &AppState,
    request: ResetPasswordRequest,
) -> Result<(), AppError> {
    password::validate_strength(&request.password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let Some((user_id, token_version, email_fingerprint)) =
        repository::consume_reset_token(&state.cache, &request.token).await?
    else {
        return Err(AppError::BadRequest(
            "código inválido ou expirado".to_owned(),
        ));
    };

    users_service::reset_password_with_code(
        &state.db,
        user_id,
        token_version,
        &email_fingerprint,
        &request.password,
    )
    .await?;

    metrics::password_reset_completed();

    // As sessões antigas já caíram pelo `token_version`; limpa os metadados.
    super::devices::clear_sessions(&state.cache, user_id).await;

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
            metrics::password_reset_blocked(scope);
            tracing::warn!(scope, retry_after_secs, "recuperação de senha bloqueada");
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
        tracing::warn!(error = %error, "falha ao registrar pedido de recuperação");
    }
}
