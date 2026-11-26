//! Rotas de item: busca, atualização, desativação e senha.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    core::{
        auth::{
            extractor::AuthUser,
            permission::{USERS_DELETE, USERS_READ, USERS_WRITE},
        },
        error::AppError,
        extract::{RequestId, ValidatedJson},
        openapi::Problem,
        state::AppState,
    },
    modules::{
        audit::service::Actor,
        users::{
            dto::{UpdateUserRequest, UserOut},
            service::{self as users_service, UserUpdate},
        },
    },
};

#[utoipa::path(
    get,
    path = "/users/{id}",
    tag = "users",
    summary = "Busca um usuário",
    description = "Requer `users:read`.",
    params(("id" = Uuid, Path, description = "ID do usuário")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Usuário encontrado", body = UserOut),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 404, description = "Usuário não encontrado", body = Problem),
    )
)]
pub(super) async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserOut>, AppError> {
    super::authorize(&state, &auth, USERS_READ).await?;

    let model = users_service::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    Ok(Json(UserOut::from_model(model)?))
}

#[utoipa::path(
    patch,
    path = "/users/{id}",
    tag = "users",
    summary = "Atualiza um usuário",
    description = "Requer `users:write`. Não permite auto-rebaixamento nem remover o último admin.",
    params(("id" = Uuid, Path, description = "ID do usuário")),
    request_body = UpdateUserRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Usuário atualizado", body = UserOut),
        (status = 400, description = "Nenhum campo informado", body = Problem),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 404, description = "Usuário não encontrado", body = Problem),
        (status = 409, description = "Conflito de e-mail, auto-alteração ou concorrência", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    request_id: RequestId,
    ValidatedJson(body): ValidatedJson<UpdateUserRequest>,
) -> Result<Json<UserOut>, AppError> {
    super::authorize(&state, &auth, USERS_WRITE).await?;

    let actor = Actor::from_auth(&auth, request_id.0.as_deref());
    let model = users_service::update(
        &state.db,
        &state.cache,
        actor,
        id,
        UserUpdate {
            name: body.name,
            email: body.email,
            role: body.role,
            active: body.active,
            version: body.version,
        },
    )
    .await?;

    Ok(Json(UserOut::from_model(model)?))
}

#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "users",
    summary = "Desativa um usuário",
    description = "Requer `users:delete`. Soft delete (`active = false`); repetir é idempotente.",
    params(("id" = Uuid, Path, description = "ID do usuário")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Usuário desativado"),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 404, description = "Usuário não encontrado", body = Problem),
        (status = 409, description = "Auto-exclusão ou último admin", body = Problem),
    )
)]
pub(super) async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    request_id: RequestId,
) -> Result<StatusCode, AppError> {
    super::authorize(&state, &auth, USERS_DELETE).await?;

    let actor = Actor::from_auth(&auth, request_id.0.as_deref());
    users_service::deactivate(&state.db, actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
