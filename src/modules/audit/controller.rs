//! Leitura da auditoria (somente admin).

use axum::{Json, extract::State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    core::{
        auth::{extractor::AuthUser, permission::AUDIT_READ},
        error::AppError,
        extract::ValidatedQuery,
        openapi::Problem,
        state::AppState,
    },
    modules::audit::{
        dto::{AuditOut, AuditPage, ListAuditQuery},
        service,
    },
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list))
}

#[utoipa::path(
    get,
    path = "/audit",
    tag = "audit",
    summary = "Lista ações privilegiadas",
    description = "Requer `audit:read`. Somente leitura, da mais recente para a mais antiga. \
                   A consulta usa o token; não confere `token_version` no banco.",
    params(ListAuditQuery),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Página de auditoria", body = AuditPage),
        (status = 400, description = "Filtro inválido", body = Problem),
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 403, description = "Sem permissão", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedQuery(query): ValidatedQuery<ListAuditQuery>,
) -> Result<Json<AuditPage>, AppError> {
    auth.require(AUDIT_READ)?;

    let (models, total) = service::list(&state.db, &query).await?;
    let items = models
        .into_iter()
        .map(AuditOut::from_model)
        .collect::<Vec<_>>();

    Ok(Json(AuditPage::new(
        items,
        query.page(),
        query.per_page(),
        total,
    )))
}
