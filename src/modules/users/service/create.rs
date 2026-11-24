//! Criação de usuário feita por administrador (não existe cadastro público).

use chrono::Utc;
use sea_orm::{DatabaseConnection, Set, TransactionTrait};
use uuid::Uuid;

use crate::{
    core::{
        auth::{password, role::Role},
        error::AppError,
    },
    database::entity::users,
    modules::{
        email::{service as email_service, templates as email_templates},
        users::{
            repository,
            service::rules::{is_unique_violation, is_valid_email, normalize_email},
        },
    },
};

pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: Role,
}

#[tracing::instrument(skip(db, input))]
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

    let password_hash = password::hash(&input.password)?;
    let now = Utc::now().fixed_offset();
    let txn = db.begin().await?;

    if repository::find_by_email(&txn, &email).await?.is_some() {
        txn.rollback().await?;
        return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
    }

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

    let model = match repository::insert(&txn, model).await {
        Ok(model) => model,
        Err(error) if is_unique_violation(&error) => {
            txn.rollback().await?;
            return Err(AppError::Conflict("e-mail já cadastrado".to_owned()));
        }
        Err(error) => {
            txn.rollback().await?;
            return Err(AppError::Database(error));
        }
    };

    // O e-mail entra no outbox na mesma transação: ou os dois existem, ou nenhum.
    if let Err(error) = email_service::enqueue(
        &txn,
        email_templates::welcome(&model.name, &model.email),
        format!("user-created:{}", model.id),
    )
    .await
    {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(model)
}
