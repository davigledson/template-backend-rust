//! Alterações feitas por administrador: dados, papel e desativação.

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, role::Role},
        error::AppError,
    },
    database::entity::users,
    modules::users::{
        repository,
        service::rules::{
            ensure_change_allowed, is_unique_violation, is_valid_email, loses_admin,
            normalize_email, parse_role,
        },
    },
};

#[derive(Debug, Default)]
pub struct UserUpdate {
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<Role>,
    pub active: Option<bool>,
}

impl UserUpdate {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.email.is_none() && self.role.is_none() && self.active.is_none()
    }
}

/// Atualização parcial feita por um administrador.
#[tracing::instrument(skip(db, input), fields(target_id = %target_id))]
pub async fn update(
    db: &DatabaseConnection,
    actor: &AuthUser,
    target_id: Uuid,
    input: UserUpdate,
) -> Result<users::Model, AppError> {
    if input.is_empty() {
        return Err(AppError::BadRequest(
            "informe ao menos um campo para atualizar".to_owned(),
        ));
    }

    let current = repository::find_by_id(db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    let current_role = parse_role(&current.role)?;
    let new_role = input.role.unwrap_or(current_role);
    let new_active = input.active.unwrap_or(current.active);

    let new_name = match input.name {
        Some(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::BadRequest("nome é obrigatório".to_owned()));
            }
            Some(name.to_owned())
        }
        None => None,
    };

    let new_email = match input.email {
        Some(email) => {
            let email = normalize_email(&email);
            if !is_valid_email(&email) {
                return Err(AppError::BadRequest("e-mail inválido".to_owned()));
            }
            Some(email)
        }
        None => None,
    };

    if let Some(email) = new_email.as_deref()
        && email != current.email
        && repository::count_by_email(db, email, target_id).await? > 0
    {
        return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
    }

    let active_admins = if loses_admin(current_role, new_role, new_active) {
        repository::count_active_admins(db).await?
    } else {
        0
    };

    ensure_change_allowed(
        actor.id,
        target_id,
        current_role,
        new_active,
        new_role,
        active_admins,
    )?;

    let bump_token_version = new_role != current_role || new_active != current.active;

    let updated = match repository::update(
        db,
        target_id,
        current.version,
        new_name,
        new_email,
        (new_role != current_role).then_some(new_role),
        (new_active != current.active).then_some(new_active),
        bump_token_version,
    )
    .await
    {
        Ok(updated) => updated,
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
        }
        Err(error) => return Err(AppError::Database(error)),
    };

    if updated == 0 {
        return Err(AppError::Conflict(
            "usuário foi alterado por outra pessoa; recarregue e tente novamente".to_owned(),
        ));
    }

    repository::find_by_id(db, target_id)
        .await?
        .ok_or_else(|| AppError::internal("usuário desapareceu após a atualização"))
}

pub async fn set_role(db: &DatabaseConnection, id: Uuid, role: Role) -> Result<(), AppError> {
    let updated = repository::set_role(db, id, role).await?;
    if updated == 0 {
        return Err(AppError::NotFound("usuário não encontrado".to_owned()));
    }
    Ok(())
}
