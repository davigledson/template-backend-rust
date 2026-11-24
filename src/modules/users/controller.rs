//! Handlers de gestão de usuários (somente administradores).
//!
//! Todas as rotas exigem `AuthUser` e chamam `require_session` para conferir
//! `active`, `token_version` e papel no banco.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    core::{
        auth::{
            extractor::AuthUser,
            permission::{USERS_DELETE, USERS_READ, USERS_WRITE},
        },
        error::AppError,
        extract::{ValidatedJson, ValidatedQuery},
        openapi::Problem,
        state::AppState,
    },
    modules::users::{
        dto::{
            ChangePasswordRequest, CreateUserRequest, ListUsersQuery, UpdateUserRequest, UserOut,
            UserPage,
        },
        service::{self as users_service, NewUser, UserUpdate},
    },
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(get, update, delete))
        .routes(routes!(change_password))
}

#[utoipa::path(
    get,
    path = "/users",
    tag = "users",
    summary = "Lista usuários",
    description = "Requer `users:read`.",
    params(ListUsersQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Página de usuários", body = UserPage),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
    )
)]
async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedQuery(query): ValidatedQuery<ListUsersQuery>,
) -> Result<Json<UserPage>, AppError> {
    authorize(&state, &auth, USERS_READ).await?;

    let (models, total) = users_service::list(&state.db, &query).await?;
    let items = models
        .into_iter()
        .map(UserOut::from_model)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(UserPage::new(
        items,
        query.page(),
        query.per_page(),
        total,
    )))
}

#[utoipa::path(
    post,
    path = "/users",
    tag = "users",
    summary = "Cria um usuário",
    description = "Requer `users:write`. Não existe cadastro público.",
    request_body = CreateUserRequest,
    params(
        ("Idempotency-Key" = Option<String>, Header, description = "Repetir a mesma chave devolve a resposta original por 24 h (até 128 caracteres ASCII)."),
    ),
    security(("bearer" = [])),
    responses(
        (status = 201, description = "Usuário criado", body = UserOut),
        (status = 400, description = "Dados inválidos", body = Problem),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 409, description = "E-mail já cadastrado", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserOut>), AppError> {
    authorize(&state, &auth, USERS_WRITE).await?;

    let model = users_service::create(
        &state.db,
        NewUser {
            name: body.name,
            email: body.email,
            password: body.password,
            role: body.role,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(UserOut::from_model(model)?)))
}

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
async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserOut>, AppError> {
    authorize(&state, &auth, USERS_READ).await?;

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
async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<UpdateUserRequest>,
) -> Result<Json<UserOut>, AppError> {
    authorize(&state, &auth, USERS_WRITE).await?;

    let model = users_service::update(
        &state.db,
        &auth,
        id,
        UserUpdate {
            name: body.name,
            email: body.email,
            role: body.role,
            active: body.active,
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
async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &auth, USERS_DELETE).await?;
    users_service::deactivate(&state.db, &auth, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/users/{id}/password",
    tag = "users",
    summary = "Redefine a senha de um usuário",
    description = "Requer `users:write`. Invalida todas as sessões do usuário.",
    params(("id" = Uuid, Path, description = "ID do usuário")),
    request_body = ChangePasswordRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Senha redefinida"),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 404, description = "Usuário não encontrado", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &auth, USERS_WRITE).await?;
    users_service::set_password(&state.db, id, &body.password).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Checa a permissão e, depois, se o token ainda vale no banco.
async fn authorize(
    state: &AppState,
    auth: &AuthUser,
    permission: crate::core::auth::permission::Permission,
) -> Result<(), AppError> {
    auth.require(permission)?;
    users_service::require_session(&state.db, auth).await?;
    Ok(())
}
