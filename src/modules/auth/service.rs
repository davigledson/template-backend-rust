//! Regras de login, refresh, logout e perfil.

use crate::{
    core::{
        auth::{password, role::Role},
        error::AppError,
        state::AppState,
    },
    database::entity::users,
    modules::{
        auth::{
            dto::{LoginRequest, RefreshRequest, TokenResponse},
            repository,
        },
        users::service as users_service,
    },
};

pub async fn login(state: &AppState, request: LoginRequest) -> Result<TokenResponse, AppError> {
    if request.email.trim().is_empty() || request.password.is_empty() {
        return Err(AppError::BadRequest(
            "e-mail e senha são obrigatórios".to_owned(),
        ));
    }

    let user = users_service::find_by_email(&state.db, &request.email)
        .await?
        .filter(|user| user.active)
        .ok_or_else(invalid_credentials)?;

    if !password::verify(&request.password, &user.password_hash)? {
        return Err(invalid_credentials());
    }

    let role = Role::parse(&user.role)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {}", user.role)))?;

    issue_tokens(state, &user, role).await
}

pub async fn refresh(state: &AppState, request: RefreshRequest) -> Result<TokenResponse, AppError> {
    let token = request.refresh_token.trim();
    if token.is_empty() {
        return Err(invalid_refresh());
    }

    let (user_id, token_version) = repository::consume(&state.cache, token)
        .await?
        .ok_or_else(invalid_refresh)?;

    let user = users_service::find_by_id(&state.db, user_id)
        .await?
        .filter(|user| user.active)
        .ok_or_else(invalid_refresh)?;

    if user.token_version != token_version {
        return Err(invalid_refresh());
    }

    let role = Role::parse(&user.role)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {}", user.role)))?;

    issue_tokens(state, &user, role).await
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
