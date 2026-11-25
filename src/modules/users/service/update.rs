//! Alterações feitas por administrador: dados, papel e desativação.

use sea_orm::{DatabaseConnection, TransactionTrait};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    core::{auth::role::Role, cache::Cache, error::AppError},
    database::entity::users,
    modules::{
        audit::service::{Action, Actor, record_user},
        users::{
            repository,
            service::rules::{
                ensure_change_allowed, is_unique_violation, is_valid_email, loses_admin,
                normalize_email, parse_role,
            },
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
#[tracing::instrument(skip(db, cache, input), fields(target_id = %target_id))]
pub async fn update(
    db: &DatabaseConnection,
    cache: &Cache,
    actor: Actor<'_>,
    target_id: Uuid,
    input: UserUpdate,
) -> Result<users::Model, AppError> {
    if input.is_empty() {
        return Err(AppError::BadRequest(
            "informe ao menos um campo para atualizar".to_owned(),
        ));
    }

    let txn = db.begin().await?;
    let current = repository::find_by_id(&txn, target_id)
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
        && repository::count_by_email(&txn, email, target_id).await? > 0
    {
        return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
    }

    let active_admins = if loses_admin(current_role, new_role, new_active) {
        repository::count_active_admins(&txn).await?
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

    let changes = diff(
        &current,
        new_name.as_deref(),
        new_email.as_deref(),
        current_role,
        new_role,
        new_active,
    );

    let bump_token_version = new_role != current_role || new_active != current.active;
    let email_changed = new_email
        .as_deref()
        .is_some_and(|email| email != current.email);

    let updated = match repository::update(
        &txn,
        target_id,
        current.version,
        new_name,
        new_email,
        (new_role != current_role).then_some(new_role),
        (new_active != current.active).then_some(new_active),
        bump_token_version,
        email_changed,
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

    let action = if new_role != current_role {
        Action::UserRoleChanged
    } else if new_active != current.active {
        Action::UserStatusChanged
    } else {
        Action::UserUpdated
    };

    if let Some(changes) = changes
        && let Err(error) = record_user(&txn, actor, action, target_id, Some(changes)).await
    {
        txn.rollback().await?;
        return Err(error);
    }

    let model = repository::find_by_id(&txn, target_id)
        .await?
        .ok_or_else(|| AppError::internal("usuário desapareceu após a atualização"))?;

    txn.commit().await?;

    if email_changed {
        super::verification::send_email_verification(db, cache, &model).await;
    }

    Ok(model)
}

/// Desativação (soft delete). Repetir a operação é idempotente.
#[tracing::instrument(skip(db), fields(target_id = %target_id))]
pub async fn deactivate(
    db: &DatabaseConnection,
    actor: Actor<'_>,
    target_id: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let current = repository::find_by_id(&txn, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    if !current.active {
        txn.rollback().await?;
        return Ok(());
    }

    let current_role = parse_role(&current.role)?;
    let active_admins = if current_role == Role::Admin {
        repository::count_active_admins(&txn).await?
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
        &txn,
        target_id,
        current.version,
        None,
        None,
        None,
        Some(false),
        true,
        false,
    )
    .await?;

    if updated == 0 {
        return Err(AppError::Conflict(
            "usuário foi alterado por outra pessoa; recarregue e tente novamente".to_owned(),
        ));
    }

    let changes = json!({ "active": { "from": true, "to": false } });
    if let Err(error) = record_user(
        &txn,
        actor,
        Action::UserStatusChanged,
        target_id,
        Some(changes),
    )
    .await
    {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(())
}

pub async fn set_role(
    db: &DatabaseConnection,
    actor: Actor<'_>,
    id: Uuid,
    role: Role,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let current = repository::find_by_id(&txn, id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    let updated = repository::set_role(&txn, id, role).await?;
    if updated == 0 {
        return Err(AppError::NotFound("usuário não encontrado".to_owned()));
    }

    let changes = json!({
        "role": { "from": current.role, "to": role.as_str() },
    });

    if let Err(error) = record_user(&txn, actor, Action::UserRoleChanged, id, Some(changes)).await {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(())
}

/// Monta o diff apenas com os campos que mudaram. `None` quando nada mudou.
fn diff(
    current: &users::Model,
    new_name: Option<&str>,
    new_email: Option<&str>,
    current_role: Role,
    new_role: Role,
    new_active: bool,
) -> Option<Value> {
    let mut map = Map::new();

    if let Some(name) = new_name
        && name != current.name
    {
        map.insert(
            "name".to_owned(),
            json!({ "from": current.name, "to": name }),
        );
    }
    if let Some(email) = new_email
        && email != current.email
    {
        map.insert(
            "email".to_owned(),
            json!({ "from": current.email, "to": email }),
        );
    }
    if new_role != current_role {
        map.insert(
            "role".to_owned(),
            json!({ "from": current_role.as_str(), "to": new_role.as_str() }),
        );
    }
    if new_active != current.active {
        map.insert(
            "active".to_owned(),
            json!({ "from": current.active, "to": new_active }),
        );
    }

    (!map.is_empty()).then_some(Value::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn model(role: &str, active: bool) -> users::Model {
        let now = Utc::now().fixed_offset();
        users::Model {
            id: Uuid::now_v7(),
            name: "Ana".to_owned(),
            email: "ana@teste.local".to_owned(),
            password_hash: "hash".to_owned(),
            role: role.to_owned(),
            active,
            token_version: 0,
            version: 0,
            email_verified_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn diff_vazio_quando_nada_muda() {
        let current = model("user", true);
        let changes = diff(
            &current,
            Some(&current.name.clone()),
            Some(&current.email.clone()),
            Role::User,
            Role::User,
            true,
        );
        assert!(changes.is_none());
    }

    #[test]
    fn diff_registra_apenas_o_que_mudou() {
        let current = model("user", true);
        let changes = diff(&current, None, None, Role::User, Role::Admin, true).expect("diff");

        assert!(changes.get("name").is_none());
        assert!(changes.get("email").is_none());
        assert_eq!(changes["role"]["from"], "user");
        assert_eq!(changes["role"]["to"], "admin");
    }
}
