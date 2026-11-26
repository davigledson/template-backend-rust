//! Regras de auditoria: vocabulário de ações e gravação append-only.
//!
//! A gravação acontece na **mesma transação** da mudança de negócio: se a
//! alteração não commitou, o registro também não existe.

use chrono::{DateTime, TimeDelta, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, Set};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, role::Role},
        error::AppError,
    },
    database::entity::audit_log,
    modules::audit::{
        dto::ListAuditQuery,
        repository::{self, Filter},
    },
};

/// Tipo de alvo registrado hoje. Novos módulos podem acrescentar outros.
pub const TARGET_USER: &str = "user";

/// Ações privilegiadas registradas pelo template.
///
/// O prefixo carrega o tipo de alvo (`user.*`); módulos futuros acrescentam
/// outros prefixos no mesmo vocabulário.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    UserCreated,
    UserAdminCreated,
    UserUpdated,
    UserRoleChanged,
    UserStatusChanged,
    UserPasswordReset,
    UserEmailVerified,
}

impl Action {
    pub const ALL: [Action; 7] = [
        Action::UserCreated,
        Action::UserAdminCreated,
        Action::UserUpdated,
        Action::UserRoleChanged,
        Action::UserStatusChanged,
        Action::UserPasswordReset,
        Action::UserEmailVerified,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserCreated => "user.created",
            Self::UserAdminCreated => "user.admin_created",
            Self::UserUpdated => "user.updated",
            Self::UserRoleChanged => "user.role_changed",
            Self::UserStatusChanged => "user.status_changed",
            Self::UserPasswordReset => "user.password_reset",
            Self::UserEmailVerified => "user.email_verified",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.as_str() == value)
    }
}

/// Quem executou a ação. `id` nulo significa CLI ou job, não usuário da API.
#[derive(Debug, Clone, Copy)]
pub struct Actor<'a> {
    pub id: Option<Uuid>,
    pub role: Option<Role>,
    pub request_id: Option<&'a str>,
}

impl<'a> Actor<'a> {
    pub const fn system() -> Actor<'static> {
        Actor {
            id: None,
            role: None,
            request_id: None,
        }
    }

    pub fn from_auth(auth: &AuthUser, request_id: Option<&'a str>) -> Self {
        Self {
            id: Some(auth.id),
            role: Some(auth.role),
            request_id,
        }
    }
}

/// Grava uma ação sobre um usuário.
pub async fn record_user<C>(
    conn: &C,
    actor: Actor<'_>,
    action: Action,
    target_id: Uuid,
    changes: Option<Value>,
) -> Result<(), AppError>
where
    C: ConnectionTrait,
{
    let entry = audit_log::ActiveModel {
        id: Set(Uuid::now_v7()),
        actor_id: Set(actor.id),
        actor_role: Set(actor.role.map(|role| role.as_str().to_owned())),
        action: Set(action.as_str().to_owned()),
        target_type: Set(TARGET_USER.to_owned()),
        target_id: Set(target_id),
        changes: Set(changes),
        request_id: Set(actor.request_id.map(str::to_owned)),
        created_at: Set(Utc::now().fixed_offset()),
    };

    Ok(repository::insert(conn, entry).await?)
}

/// Consulta paginada com filtros validados.
pub async fn list(
    db: &DatabaseConnection,
    query: &ListAuditQuery,
) -> Result<(Vec<audit_log::Model>, u64), AppError> {
    if let Some(action) = query.action.as_deref()
        && Action::parse(action).is_none()
    {
        let known = Action::ALL
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::BadRequest(format!(
            "action desconhecida; use uma de: {known}"
        )));
    }

    let filter = Filter {
        actor_id: query.actor_id,
        target_id: query.target_id,
        action: query.action.as_deref(),
        from: query.from,
        to: query.to,
    };

    Ok(repository::list(db, query.offset(), query.per_page(), &filter).await?)
}

/// Remove registros mais antigos que a retenção configurada.
///
/// `retention_days = 0` desliga o expurgo. O corte é calculado no relógio do
/// processo do worker; a operação é idempotente.
pub async fn purge(db: &DatabaseConnection, retention_days: u64) -> Result<u64, AppError> {
    let Some(cutoff) = retention_cutoff(retention_days) else {
        return Ok(0);
    };

    Ok(repository::delete_before(db, cutoff).await?)
}

fn retention_cutoff(retention_days: u64) -> Option<DateTime<Utc>> {
    if retention_days == 0 {
        return None;
    }

    let days = i64::try_from(retention_days).ok()?;
    let delta = TimeDelta::try_days(days)?;
    Utc::now().checked_sub_signed(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acoes_ida_e_volta() {
        for action in Action::ALL {
            assert_eq!(Action::parse(action.as_str()), Some(action));
        }
        assert_eq!(Action::parse("user.apagado"), None);
    }

    #[test]
    fn retencao_zero_desliga_e_valores_invalidos_nao_cortam() {
        assert!(retention_cutoff(0).is_none());
        assert!(retention_cutoff(365).is_some());
        assert!(retention_cutoff(u64::MAX).is_none());
    }

    #[test]
    fn ator_de_sistema_nao_tem_id() {
        let actor = Actor::system();
        assert!(actor.id.is_none());
        assert!(actor.role.is_none());
    }
}
