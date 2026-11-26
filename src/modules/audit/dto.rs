//! DTOs de auditoria.
//!
//! O registro é exposto como histórico: `changes` traz só os campos alterados,
//! nunca segredo. `actorRole` é o papel no momento da ação, não o atual.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{core::auth::role::Role, database::entity::audit_log};

const DEFAULT_PAGE: u64 = 1;
const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

fn default_page() -> u64 {
    DEFAULT_PAGE
}

fn default_per_page() -> u64 {
    DEFAULT_PER_PAGE
}

/// Uma ação privilegiada registrada pela API.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditOut {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub actor_role: Option<Role>,
    /// `user.created`, `user.updated`, `user.role_changed`,
    /// `user.status_changed`, `user.password_reset` ou `user.admin_created`.
    pub action: String,
    pub target_type: String,
    pub target_id: Uuid,
    /// Campos alterados (`{ "role": { "from": "user", "to": "admin" } }`).
    pub changes: Option<serde_json::Value>,
    /// Cabeçalho `x-request-id` da requisição original.
    pub request_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl AuditOut {
    pub fn from_model(model: audit_log::Model) -> Self {
        Self {
            id: model.id,
            actor_id: model.actor_id,
            actor_role: model.actor_role.and_then(|role| Role::parse(&role)),
            action: model.action,
            target_type: model.target_type,
            target_id: model.target_id,
            changes: model.changes,
            request_id: model.request_id,
            created_at: model.created_at.with_timezone(&Utc),
        }
    }
}

/// Página de auditoria.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    pub items: Vec<AuditOut>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
}

impl AuditPage {
    pub fn new(items: Vec<AuditOut>, page: u64, per_page: u64, total: u64) -> Self {
        Self {
            items,
            page,
            per_page,
            total,
        }
    }
}

/// Filtros da auditoria. Datas em RFC 3339 (`2026-11-24T00:00:00Z`).
#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListAuditQuery {
    #[serde(default = "default_page")]
    #[validate(range(min = 1, max = 1_000_000))]
    #[param(minimum = 1, maximum = 1000000, default = 1)]
    pub page: u64,
    #[serde(default = "default_per_page")]
    #[validate(range(min = 1, max = 100))]
    #[param(minimum = 1, maximum = 100, default = 20)]
    pub per_page: u64,
    /// Quem executou a ação. Ausente inclui CLI e jobs.
    pub actor_id: Option<Uuid>,
    /// Sobre quem a ação foi executada.
    pub target_id: Option<Uuid>,
    /// Ação exata (`user.created`, `user.role_changed`, ...).
    #[validate(length(max = 40))]
    pub action: Option<String>,
    /// Início do período (RFC 3339).
    pub from: Option<DateTime<Utc>>,
    /// Fim do período (RFC 3339).
    pub to: Option<DateTime<Utc>>,
}

impl ListAuditQuery {
    pub fn page(&self) -> u64 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u64 {
        self.per_page.clamp(1, MAX_PER_PAGE)
    }

    pub fn offset(&self) -> u64 {
        self.page()
            .saturating_sub(1)
            .saturating_mul(self.per_page())
            .min(i64::MAX as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(page: u64, per_page: u64) -> ListAuditQuery {
        ListAuditQuery {
            page,
            per_page,
            actor_id: None,
            target_id: None,
            action: None,
            from: None,
            to: None,
        }
    }

    #[test]
    fn paginacao_nunca_estoura_limites() {
        let q = query(0, 0);
        assert_eq!(q.page(), 1);
        assert_eq!(q.per_page(), 1);
        assert_eq!(q.offset(), 0);

        let q = query(3, 500);
        assert_eq!(q.page(), 3);
        assert_eq!(q.per_page(), 100);
        assert_eq!(q.offset(), 200);
    }

    #[test]
    fn offset_maximo_nao_estoura() {
        let q = query(u64::MAX, 100);
        assert_eq!(q.offset(), i64::MAX as u64);
    }
}
