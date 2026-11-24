//! Regras do módulo `users`.
//!
//! É a porta pública para os outros módulos (ex.: `auth` busca usuário aqui).

use chrono::Utc;
use sea_orm::{DatabaseConnection, DbErr, Set};
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, password, role::Role},
        error::AppError,
    },
    database::entity::users,
    modules::users::{dto::ListUsersQuery, repository},
};

pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: Role,
}

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

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<users::Model>, AppError> {
    Ok(repository::find_by_id(db, id).await?)
}

pub async fn find_by_email(
    db: &DatabaseConnection,
    email: &str,
) -> Result<Option<users::Model>, AppError> {
    Ok(repository::find_by_email(db, &normalize_email(email)).await?)
}

pub async fn list(
    db: &DatabaseConnection,
    query: &ListUsersQuery,
) -> Result<(Vec<users::Model>, u64), AppError> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty());

    Ok(repository::list(
        db,
        query.offset(),
        query.per_page(),
        search,
        query.role,
        query.active,
    )
    .await?)
}

pub async fn create(db: &DatabaseConnection, input: NewUser) -> Result<users::Model, AppError> {
    let name = input.name.trim();
    let email = normalize_email(&input.email);

    if name.is_empty() {
        return Err(AppError::BadRequest("nome é obrigatório".to_owned()));
    }
    if !is_valid_email(&email) {
        return Err(AppError::BadRequest("e-mail inválido".to_owned()));
    }
    password::validate_strength(&input.password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    if repository::find_by_email(db, &email).await?.is_some() {
        return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
    }

    let password_hash = password::hash(&input.password)?;
    let now = Utc::now().fixed_offset();
    let model = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(name.to_owned()),
        email: Set(email),
        password_hash: Set(password_hash),
        role: Set(input.role.as_str().to_owned()),
        active: Set(true),
        token_version: Set(0),
        version: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
    };

    match repository::insert(db, model).await {
        Ok(model) => Ok(model),
        Err(error) if is_unique_violation(&error) => {
            Err(AppError::Conflict("e-mail já cadastrado".to_owned()))
        }
        Err(error) => Err(AppError::Database(error)),
    }
}

/// Atualização parcial feita por um administrador.
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

pub async fn set_role(db: &DatabaseConnection, id: Uuid, role: Role) -> Result<(), AppError> {
    let updated = repository::set_role(db, id, role).await?;
    if updated == 0 {
        return Err(AppError::NotFound("usuário não encontrado".to_owned()));
    }
    Ok(())
}

pub async fn set_password(
    db: &DatabaseConnection,
    id: Uuid,
    new_password: &str,
) -> Result<(), AppError> {
    password::validate_strength(new_password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let password_hash = password::hash(new_password)?;
    let updated = repository::set_password(db, id, &password_hash).await?;
    if updated == 0 {
        return Err(AppError::NotFound("usuário não encontrado".to_owned()));
    }
    Ok(())
}

pub async fn count_active_admins(db: &DatabaseConnection) -> Result<u64, AppError> {
    Ok(repository::count_active_admins(db).await?)
}

/// Garante que o token ainda representa o estado atual do usuário.
///
/// Usado por rotas sensíveis (`/auth/me`, gestão de usuários). Verifica
/// `active`, `token_version` e o papel no banco.
pub async fn require_session(
    db: &DatabaseConnection,
    auth: &AuthUser,
) -> Result<users::Model, AppError> {
    let model = repository::find_by_id(db, auth.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("sessão inválida".to_owned()))?;

    if !model.active
        || model.token_version != auth.token_version
        || model.role != auth.role.as_str()
    {
        return Err(AppError::Unauthorized(
            "sessão expirada; faça login novamente".to_owned(),
        ));
    }

    Ok(model)
}

fn loses_admin(current_role: Role, new_role: Role, new_active: bool) -> bool {
    current_role == Role::Admin && (new_role != Role::Admin || !new_active)
}

/// Impede auto-rebaixamento/autoexclusão e a remoção do último admin ativo.
fn ensure_change_allowed(
    actor_id: Uuid,
    target_id: Uuid,
    current_role: Role,
    new_active: bool,
    new_role: Role,
    active_admins: u64,
) -> Result<(), AppError> {
    if actor_id == target_id {
        if !new_active {
            return Err(AppError::Conflict(
                "não é possível desativar o próprio usuário".to_owned(),
            ));
        }
        if new_role != current_role {
            return Err(AppError::Conflict(
                "não é possível alterar o próprio papel".to_owned(),
            ));
        }
    }

    if loses_admin(current_role, new_role, new_active) && active_admins <= 1 {
        return Err(AppError::Conflict(
            "não é possível remover o último administrador ativo".to_owned(),
        ));
    }

    Ok(())
}

fn parse_role(value: &str) -> Result<Role, AppError> {
    Role::parse(value)
        .ok_or_else(|| AppError::internal(format!("papel inválido no banco: {value}")))
}

fn is_valid_email(email: &str) -> bool {
    let (local, domain) = match email.split_once('@') {
        Some(parts) => parts,
        None => return false,
    };

    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !email.chars().any(char::is_whitespace)
        && email.len() <= 254
}

fn is_unique_violation(error: &DbErr) -> bool {
    error
        .to_string()
        .contains("duplicate key value violates unique constraint")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_email() {
        assert_eq!(normalize_email("  A@B.COM "), "a@b.com");
    }

    #[test]
    fn valida_email_basico() {
        assert!(is_valid_email("a@b.com"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("@b.com"));
        assert!(!is_valid_email("a@"));
        assert!(!is_valid_email("a b@c.com"));
    }

    #[test]
    fn nao_permite_auto_desativacao() {
        let id = Uuid::now_v7();
        let error = ensure_change_allowed(id, id, Role::Admin, false, Role::Admin, 2);
        assert!(matches!(error, Err(AppError::Conflict(_))));
    }

    #[test]
    fn nao_permite_auto_rebaixamento() {
        let id = Uuid::now_v7();
        let error = ensure_change_allowed(id, id, Role::Admin, true, Role::User, 2);
        assert!(matches!(error, Err(AppError::Conflict(_))));
    }

    #[test]
    fn nao_permite_remover_ultimo_admin() {
        let actor = Uuid::now_v7();
        let target = Uuid::now_v7();

        let demote = ensure_change_allowed(actor, target, Role::Admin, true, Role::User, 1);
        assert!(matches!(demote, Err(AppError::Conflict(_))));

        let deactivate = ensure_change_allowed(actor, target, Role::Admin, false, Role::Admin, 1);
        assert!(matches!(deactivate, Err(AppError::Conflict(_))));
    }

    #[test]
    fn permite_alterar_quando_ha_outro_admin() {
        let actor = Uuid::now_v7();
        let target = Uuid::now_v7();

        assert!(ensure_change_allowed(actor, target, Role::Admin, true, Role::User, 2).is_ok());
        assert!(ensure_change_allowed(actor, target, Role::Admin, false, Role::Admin, 2).is_ok());
        assert!(ensure_change_allowed(actor, target, Role::User, false, Role::User, 1).is_ok());
    }

    #[test]
    fn admin_pode_editar_a_si_mesmo_sem_mudar_papel() {
        let id = Uuid::now_v7();
        assert!(ensure_change_allowed(id, id, Role::Admin, true, Role::Admin, 1).is_ok());
    }
}
