//! Handlers de autenticação.

use axum::{Json, extract::State, http::StatusCode};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    core::{
        auth::{extractor::AuthUser, permission::PROFILE_WRITE},
        client_ip::ClientIp,
        error::AppError,
        extract::ValidatedJson,
        openapi::Problem,
        state::AppState,
    },
    modules::{
        auth::{
            dto::{
                ChangeOwnPasswordRequest, LoginRequest, RefreshRequest, TokenResponse,
                UpdateProfileRequest,
            },
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
        .routes(routes!(update_me))
        .routes(routes!(change_my_password))
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
        (status = 429, description = "Muitas tentativas de login", body = Problem),
    )
)]
async fn login(
    State(state): State<AppState>,
    client_ip: ClientIp,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(
        service::login(&state, body, client_ip.as_str()).await?,
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

#[utoipa::path(
    patch,
    path = "/auth/me",
    tag = "auth",
    summary = "Atualiza o próprio perfil",
    description = "Altera apenas o nome. Papel e e-mail são geridos por um administrador.",
    security(("bearer" = [])),
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Perfil atualizado", body = UserOut),
        (status = 400, description = "Nome vazio", body = Problem),
        (status = 401, description = "Não autenticado ou sessão expirada", body = Problem),
        (status = 403, description = "Permissão insuficiente", body = Problem),
        (status = 409, description = "Conflito de concorrência", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
async fn update_me(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<UpdateProfileRequest>,
) -> Result<Json<UserOut>, AppError> {
    auth.require(PROFILE_WRITE)?;
    let user = users_service::update_profile(&state.db, &auth, &body.name).await?;
    Ok(Json(UserOut::from_model(user)?))
}

#[utoipa::path(
    put,
    path = "/auth/me/password",
    tag = "auth",
    summary = "Troca a própria senha",
    description = "Exige a senha atual e encerra todas as sessões do usuário.",
    security(("bearer" = [])),
    request_body = ChangeOwnPasswordRequest,
    responses(
        (status = 204, description = "Senha alterada; faça login novamente"),
        (status = 400, description = "Nova senha igual à atual ou fraca", body = Problem),
        (status = 401, description = "Senha atual incorreta ou sessão expirada", body = Problem),
        (status = 403, description = "Permissão insuficiente", body = Problem),
        (status = 409, description = "Conflito de concorrência", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
async fn change_my_password(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<ChangeOwnPasswordRequest>,
) -> Result<StatusCode, AppError> {
    auth.require(PROFILE_WRITE)?;
    users_service::change_own_password(
        &state.db,
        &auth,
        &body.current_password,
        &body.new_password,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
