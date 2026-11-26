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
    modules::{
        audit::{
            dto::{AuditOut, AuditPage, ListAuditQuery},
            service,
        },
        users::service as users_service,
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
    description = "Requer `audit:read` e sessão válida no banco (ativo, `token_version` e papel). \
                   Somente leitura, da mais recente para a mais antiga.",
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
    // A escrita em `audit_log` é feita pelos outros módulos; a leitura confere
    // a sessão no banco para um token rebaixado/desativado não ver o histórico.
    users_service::require_session(&state.db, &auth).await?;

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
