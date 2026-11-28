//! Senhas: redefinição pelo administrador, recuperação por código e troca pelo
//! próprio usuário.

use sea_orm::{DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use crate::{
    core::{
        auth::{extractor::AuthUser, hash::email_fingerprint, password},
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

    let password_hash = password::hash_async(new_password.to_owned()).await?;
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

    if !password::verify_async(current_password.to_owned(), user.password_hash.clone()).await? {
        return Err(AppError::Unauthorized("senha atual incorreta".to_owned()));
    }
    if current_password == new_password {
        return Err(AppError::BadRequest(
            "a nova senha deve ser diferente da atual".to_owned(),
        ));
    }
    password::validate_strength(new_password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let password_hash = password::hash_async(new_password.to_owned()).await?;
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

/// Recuperação por código enviado ao e-mail (usuário não autenticado).
///
/// O código carrega o `token_version` e o fingerprint do e-mail do momento da
/// emissão: se a senha, o papel ou o e-mail mudaram depois, ele deixa de valer.
/// O novo `token_version` derruba todas as sessões existentes.
pub async fn reset_password_with_code(
    db: &DatabaseConnection,
    user_id: Uuid,
    expected_token_version: i32,
    expected_email_fingerprint: &str,
    new_password: &str,
) -> Result<(), AppError> {
    password::validate_strength(new_password)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    let password_hash = password::hash_async(new_password.to_owned()).await?;
    let txn = db.begin().await?;

    let user = repository::find_by_id(&txn, user_id)
        .await?
        .filter(|user| {
            user.active
                && user.token_version == expected_token_version
                && email_fingerprint(&user.email) == expected_email_fingerprint
        })
        .ok_or_else(invalid_code)?;

    let updated = repository::set_password(&txn, user.id, &password_hash).await?;
    if updated == 0 {
        txn.rollback().await?;
        return Err(invalid_code());
    }

    // Receber o código no endereço prova a posse da caixa: confirma o e-mail.
    repository::set_email_verified(&txn, user.id).await?;

    let user = repository::find_by_id(&txn, user_id)
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

    txn.commit().await?;
    Ok(())
}

fn invalid_code() -> AppError {
    AppError::BadRequest("código inválido ou expirado".to_owned())
}
