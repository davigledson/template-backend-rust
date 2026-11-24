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
    modules::users::repository,
};

pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: Role,
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
}
