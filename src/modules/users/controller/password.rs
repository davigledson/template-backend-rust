//! Redefinição de senha feita por administrador.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, permission::USERS_WRITE},
        error::AppError,
        extract::{RequestId, ValidatedJson},
        openapi::Problem,
        state::AppState,
    },
    modules::{
        audit::service::Actor,
        auth::service as auth_service,
        users::{dto::ChangePasswordRequest, service as users_service},
    },
};

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
pub(super) async fn change(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    request_id: RequestId,
    ValidatedJson(body): ValidatedJson<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    super::authorize(&state, &auth, USERS_WRITE).await?;

    let actor = Actor::from_auth(&auth, request_id.0.as_deref());
    users_service::set_password(&state.db, actor, id, &body.password).await?;
    auth_service::clear_sessions(&state, id).await;
    Ok(StatusCode::NO_CONTENT)
}
