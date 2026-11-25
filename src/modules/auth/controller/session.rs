//! Sessão: login, refresh e logout.

use axum::{Json, extract::State, http::StatusCode};

use crate::{
    core::{
        client_ip::ClientIp,
        error::AppError,
        extract::{UserAgent, ValidatedJson},
        openapi::Problem,
        state::AppState,
    },
    modules::auth::{
        dto::{LoginRequest, RefreshRequest, TokenResponse},
        service,
    },
};

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    summary = "Autentica e emite os tokens",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Tokens emitidos", body = TokenResponse),
        (status = 400, description = "JSON inválido", body = Problem),
        (status = 401, description = "Credenciais inválidas", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
        (status = 429, description = "Muitas tentativas de login", body = Problem),
    )
)]
pub(super) async fn login(
    State(state): State<AppState>,
    client_ip: ClientIp,
    user_agent: UserAgent,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(
        service::login(&state, body, client_ip.as_str(), user_agent.0.as_deref()).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/auth/refresh",
    tag = "auth",
    summary = "Rotaciona o refresh token",
    description = "O refresh token usado é revogado; um novo par é emitido.",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Novos tokens", body = TokenResponse),
        (status = 401, description = "Refresh token inválido ou expirado", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn refresh(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(service::refresh(&state, body).await?))
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    summary = "Revoga o refresh token",
    request_body = RefreshRequest,
    responses(
        (status = 204, description = "Sessão encerrada"),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn logout(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<StatusCode, AppError> {
    service::logout(&state, body).await?;
    Ok(StatusCode::NO_CONTENT)
}
