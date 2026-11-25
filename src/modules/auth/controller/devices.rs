//! Sessões por dispositivo: listar e encerrar.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    core::{auth::extractor::AuthUser, error::AppError, openapi::Problem, state::AppState},
    modules::auth::{dto::SessionList, service},
};

#[utoipa::path(
    get,
    path = "/auth/sessions",
    tag = "auth",
    summary = "Lista as sessões ativas",
    description = "Uma entrada por família de refresh, da mais recente para a \
                   mais antiga. `current` marca a sessão desta requisição.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Sessões ativas", body = SessionList),
        (status = 401, description = "Não autenticado ou sessão expirada", body = Problem),
    )
)]
pub(super) async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SessionList>, AppError> {
    Ok(Json(service::list_sessions(&state, &auth).await?))
}

#[utoipa::path(
    delete,
    path = "/auth/sessions/{familyId}",
    tag = "auth",
    summary = "Encerra uma sessão",
    description = "Revoga a família de refresh. A sessão atual deve sair por \
                   `POST /auth/logout`.",
    security(("bearer" = [])),
    params(
        ("familyId" = Uuid, Path, description = "Identificador da sessão"),
    ),
    responses(
        (status = 204, description = "Sessão encerrada"),
        (status = 400, description = "Tentativa de encerrar a sessão atual", body = Problem),
        (status = 401, description = "Não autenticado ou sessão expirada", body = Problem),
        (status = 404, description = "Sessão não encontrada", body = Problem),
    )
)]
pub(super) async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(family_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    service::revoke_session(&state, &auth, family_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
