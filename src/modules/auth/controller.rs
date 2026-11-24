//! Handlers de autenticação.

use axum::{Json, extract::State, http::StatusCode};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    core::{
        auth::extractor::AuthUser, error::AppError, extract::ValidatedJson, openapi::Problem,
        state::AppState,
    },
    modules::{
        auth::{
            dto::{LoginRequest, RefreshRequest, TokenResponse},
            service,
        },
        users::{dto::UserOut, service as users_service},
    },
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(refresh))
        .routes(routes!(logout))
        .routes(routes!(me))
}

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
    )
)]
async fn login(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(service::login(&state, body).await?))
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
async fn refresh(
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
async fn logout(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<StatusCode, AppError> {
    service::logout(&state, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    summary = "Perfil do usuário autenticado",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Perfil", body = UserOut),
        (status = 401, description = "Não autenticado ou sessão expirada", body = Problem),
    )
)]
async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<Json<UserOut>, AppError> {
    let user = users_service::require_session(&state.db, &auth).await?;
    Ok(Json(UserOut::from_model(user)?))
}
