//! Perfil do próprio usuário: leitura, nome e senha.

use axum::{Json, extract::State, http::StatusCode};

use crate::{
    core::{
        auth::{extractor::AuthUser, permission::PROFILE_WRITE},
        error::AppError,
        extract::ValidatedJson,
        openapi::Problem,
        state::AppState,
    },
    modules::{
        auth::dto::{ChangeOwnPasswordRequest, UpdateProfileRequest},
        users::{dto::UserOut, service as users_service},
    },
};

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
pub(super) async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserOut>, AppError> {
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
pub(super) async fn update_me(
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
pub(super) async fn change_my_password(
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
