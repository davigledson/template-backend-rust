//! Regras de login, refresh, logout e perfil.

use crate::{
    core::{
        auth::{
            password,
            rate_limit::{LimitDecision, LoginRateLimiter},
            role::Role,
        },
        error::AppError,
        state::AppState,
    },
    database::entity::users,
    modules::{
        auth::{
            dto::{LoginRequest, RefreshRequest, TokenResponse},
            repository::{self, ConsumeOutcome},
        },
        users::service as users_service,
    },
};

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
        "conta",
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
    scope: &str,
) -> Result<(), AppError> {
    match limiter.check(key, max_attempts).await {
        Ok(LimitDecision::Allowed) => Ok(()),
        Ok(LimitDecision::Blocked { retry_after_secs }) => {
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

async fn issue_tokens(
    state: &AppState,
    user: &users::Model,
    role: Role,
    family_id: uuid::Uuid,
) -> Result<TokenResponse, AppError> {
    let access_token = state
        .jwt
        .issue_access(user.id, &user.email, role, user.token_version)?;

    let refresh_token = repository::generate_token();
    repository::store(
        &state.cache,
        &refresh_token,
        user.id,
        user.token_version,
        family_id,
        state.config.jwt.refresh_ttl(),
    )
    .await?;

    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: state.jwt.access_ttl_secs(),
    })
}

fn invalid_credentials() -> AppError {
    AppError::Unauthorized("credenciais inválidas".to_owned())
}

fn invalid_refresh() -> AppError {
    AppError::Unauthorized("refresh token inválido ou expirado".to_owned())
}
