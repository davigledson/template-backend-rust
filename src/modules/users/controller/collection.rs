//! Rotas de coleção: listagem e criação.

use crate::{
    core::{
        auth::{
            extractor::AuthUser,
            permission::{USERS_READ, USERS_WRITE},
        },
        error::AppError,
        extract::{RequestId, ValidatedJson, ValidatedQuery},
        openapi::Problem,
        state::AppState,
    },
    modules::{
        audit::service::Actor,
        users::{
            dto::{CreateUserRequest, ListUsersQuery, UserOut, UserPage},
            service::{self as users_service, NewUser},
        },
    },
};
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header::LOCATION},
    response::IntoResponse,
};

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
pub(super) async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedQuery(query): ValidatedQuery<ListUsersQuery>,
) -> Result<Json<UserPage>, AppError> {
    super::authorize(&state, &auth, USERS_READ).await?;

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
    description = "Requer `users:write`. Não existe cadastro público. Exige `Idempotency-Key`.",
    request_body = CreateUserRequest,
    params(
        ("Idempotency-Key" = String, Header, description = "Obrigatório. Repetir a mesma chave devolve a resposta original por 24 h (até 128 caracteres ASCII)."),
    ),
    security(("bearer" = [])),
    responses(
        (status = 201, description = "Usuário criado; `Location` aponta para o recurso", body = UserOut),
        (status = 400, description = "Dados inválidos ou `Idempotency-Key` ausente", body = Problem),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 409, description = "E-mail já cadastrado", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    request_id: RequestId,
    ValidatedJson(body): ValidatedJson<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::authorize(&state, &auth, USERS_WRITE).await?;

    let actor = Actor::from_auth(&auth, request_id.0.as_deref());
    let model = users_service::create(
        &state.db,
        actor,
        NewUser {
            name: body.name,
            email: body.email,
            password: body.password,
            role: body.role,
        },
    )
    .await?;

    let location = format!("/api/v1/users/{}", model.id);
    Ok((
        StatusCode::CREATED,
        [(LOCATION, location)],
        Json(UserOut::from_model(model)?),
    ))
}
