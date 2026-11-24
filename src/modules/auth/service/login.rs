//! Login: valida credenciais, aplica rate limit e emite o par de tokens.

use crate::{
    core::{
        auth::{
            password,
            rate_limit::{LimitDecision, LoginRateLimiter},
            role::Role,
        },
        error::AppError,
        metrics,
        state::AppState,
    },
    modules::{
        auth::{
            dto::{LoginRequest, TokenResponse},
            repository,
            service::tokens::issue_tokens,
        },
        users::service as users_service,
    },
};

#[tracing::instrument(skip(state, request, client_ip))]
pub async fn login(
    state: &AppState,
    request: LoginRequest,
    client_ip: &str,
) -> Result<TokenResponse, AppError> {
    if request.email.trim().is_empty() || request.password.is_empty() {
        return Err(AppError::BadRequest(
            "e-mail e senha são obrigatórios".to_owned(),
        ));
    }

    let email = users_service::normalize_email(&request.email);
    let limiter = LoginRateLimiter::new(&state.cache, state.config.rate_limit.window());
    let account_key = LoginRateLimiter::account_key(&email);
    let ip_key = LoginRateLimiter::ip_key(client_ip);

    enforce_limit(
        &limiter,
        &account_key,
        state.config.rate_limit.login_account_attempts,
        "account",
    )
    .await?;
    enforce_limit(
        &limiter,
        &ip_key,
        state.config.rate_limit.login_ip_attempts,
        "ip",
    )
    .await?;

    let candidate = users_service::find_by_email(&state.db, &email).await?;
    let password_ok = match candidate.as_ref() {
        Some(user) if user.active => password::verify(&request.password, &user.password_hash)?,
        _ => false,
    };

    let Some(user) = candidate.filter(|_| password_ok) else {
        metrics::login_failure();
        register_failure(&limiter, &account_key, &ip_key).await;
        return Err(invalid_credentials());
    };

    // Login válido zera as falhas da conta; as do IP seguem até a janela expirar.
    if let Err(error) = limiter.clear(&account_key).await {
        tracing::warn!(error = %error, "falha ao limpar o rate limit da conta");
    }

    let role = Role::parse(&user.role)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {}", user.role)))?;

    issue_tokens(state, &user, role, repository::new_family_id()).await
}

async fn enforce_limit(
    limiter: &LoginRateLimiter<'_>,
    key: &str,
    max_attempts: u32,
    scope: &'static str,
) -> Result<(), AppError> {
    match limiter.check(key, max_attempts).await {
        Ok(LimitDecision::Allowed) => Ok(()),
        Ok(LimitDecision::Blocked { retry_after_secs }) => {
            metrics::login_blocked(scope);
            tracing::warn!(scope, retry_after_secs, "login bloqueado por rate limit");
            Err(AppError::TooManyRequests { retry_after_secs })
        }
        Err(error) => {
            // Fail-open: Redis fora do ar não pode impedir o login.
            tracing::warn!(error = %error, scope, "rate limit indisponível; seguindo sem limite");
            Ok(())
        }
    }
}

async fn register_failure(limiter: &LoginRateLimiter<'_>, account_key: &str, ip_key: &str) {
    if let Err(error) = limiter.register_failure(account_key).await {
        tracing::warn!(error = %error, "falha ao registrar tentativa de login");
    }
    if let Err(error) = limiter.register_failure(ip_key).await {
        tracing::warn!(error = %error, "falha ao registrar tentativa de login por ip");
    }
}

fn invalid_credentials() -> AppError {
    AppError::Unauthorized("credenciais inválidas".to_owned())
}
