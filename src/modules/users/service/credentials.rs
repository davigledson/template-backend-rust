//! Senhas: redefinição pelo administrador e troca pelo próprio usuário.

use sea_orm::{DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, password},
        error::AppError,
    },
    modules::{
        audit::service::{Action, Actor, record_user},
        email::{service as email_service, templates as email_templates},
        users::{repository, service::session::require_session},
    },
};

/// Redefinição feita por um administrador. Invalida as sessões do usuário.
pub async fn set_password(
    db: &DatabaseConnection,
    actor: Actor<'_>,
    id: Uuid,
    new_password: &str,
) -> Result<(), AppError> {
    password::validate_strength(new_password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let password_hash = password::hash(new_password)?;
    let txn = db.begin().await?;

    let updated = repository::set_password(&txn, id, &password_hash).await?;
    if updated == 0 {
        txn.rollback().await?;
        return Err(AppError::NotFound("usuário não encontrado".to_owned()));
    }

    let user = repository::find_by_id(&txn, id)
        .await?
        .ok_or_else(|| AppError::internal("usuário desapareceu após a troca de senha"))?;

    if let Err(error) = email_service::enqueue(
        &txn,
        email_templates::password_changed(&user.name, &user.email),
        format!("password-changed:{}:{}", user.id, user.token_version),
    )
    .await
    {
        txn.rollback().await?;
        return Err(error);
    }

    let changes = serde_json::json!({
        "tokenVersion": { "to": user.token_version },
    });
    if let Err(error) = record_user(&txn, actor, Action::UserPasswordReset, id, Some(changes)).await
    {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(())
}

/// Troca a própria senha após conferir a senha atual.
///
/// A troca incrementa `token_version`: **todas** as sessões caem, inclusive a
/// que fez a troca, e o usuário precisa entrar de novo.
pub async fn change_own_password(
    db: &DatabaseConnection,
    auth: &AuthUser,
    current_password: &str,
    new_password: &str,
) -> Result<(), AppError> {
    let user = require_session(db, auth).await?;

    if !password::verify(current_password, &user.password_hash)? {
        return Err(AppError::Unauthorized("senha atual incorreta".to_owned()));
    }
    if password::verify(new_password, &user.password_hash)? {
        return Err(AppError::BadRequest(
            "a nova senha deve ser diferente da atual".to_owned(),
        ));
    }
    password::validate_strength(new_password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let password_hash = password::hash(new_password)?;
    let txn = db.begin().await?;

    let updated = repository::set_password(&txn, user.id, &password_hash).await?;
    if updated == 0 {
        txn.rollback().await?;
        return Err(AppError::Conflict(
            "perfil foi alterado por outra sessão; recarregue e tente novamente".to_owned(),
        ));
    }

    let user = repository::find_by_id(&txn, user.id)
        .await?
        .ok_or_else(|| AppError::internal("usuário desapareceu após a troca de senha"))?;

    if let Err(error) = email_service::enqueue(
        &txn,
        email_templates::password_changed_by_user(&user.name, &user.email),
        format!("password-changed-self:{}:{}", user.id, user.token_version),
    )
    .await
    {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(())
}
