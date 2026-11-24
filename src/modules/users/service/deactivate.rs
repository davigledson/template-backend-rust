//! Desativação de usuário (soft delete).

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, role::Role},
        error::AppError,
    },
    modules::users::{
        repository,
        service::rules::{ensure_change_allowed, parse_role},
    },
};

/// Desativação (soft delete). Repetir a operação é idempotente.
pub async fn deactivate(
    db: &DatabaseConnection,
    actor: &AuthUser,
    target_id: Uuid,
) -> Result<(), AppError> {
    let current = repository::find_by_id(db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    if !current.active {
        return Ok(());
    }

    let current_role = parse_role(&current.role)?;
    let active_admins = if current_role == Role::Admin {
        repository::count_active_admins(db).await?
    } else {
        0
    };

    ensure_change_allowed(
        actor.id,
        target_id,
        current_role,
        false,
        current_role,
        active_admins,
    )?;

    let updated = repository::update(
        db,
        target_id,
        current.version,
        None,
        None,
        None,
        Some(false),
        true,
    )
    .await?;

    if updated == 0 {
        return Err(AppError::Conflict(
            "usuário foi alterado por outra pessoa; recarregue e tente novamente".to_owned(),
        ));
    }

    Ok(())
}
