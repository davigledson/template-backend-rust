//! Confirmação de e-mail: emissão do token e marcação de verificado.
//!
//! Usada por `users` (criação/troca de e-mail) e por `auth` (confirmação e
//! reenvio), sempre através desta porta.

use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::{
    core::{
        auth::{hash::email_fingerprint, verification_token},
        cache::Cache,
        error::AppError,
        mail::mask_email,
    },
    database::entity::users,
    modules::{
        audit::service::{Action, Actor, record_user},
        email::{service as email_service, templates as email_templates},
        users::repository,
    },
};

/// Envia (ou reenvia) o código de confirmação. Best-effort: falha de e-mail ou
/// Redis não pode impedir a criação do usuário.
pub async fn send_email_verification(db: &DatabaseConnection, cache: &Cache, user: &users::Model) {
    let token =
        match verification_token::issue(cache, user.id, user.token_version, &user.email).await {
            Ok(token) => token,
            Err(error) => {
                tracing::warn!(error = %error, "falha ao emitir o token de confirmação");
                return;
            }
        };

    let dedupe_key = format!("email-verify:{}:{}", user.id, &token[..8]);
    let message = email_templates::verify_email(&user.name, &user.email, &token);

    if let Err(error) = email_service::enqueue(db, message, dedupe_key).await {
        tracing::warn!(
            error = %error,
            to = %mask_email(&user.email),
            "falha ao enfileirar o e-mail de confirmação"
        );
    }
}

/// Marca o e-mail como confirmado depois de validar que o código ainda
/// corresponde ao estado atual da conta.
///
/// O código só vale se o usuário segue ativo, o `token_version` não mudou e o
/// e-mail é o mesmo da emissão. Caso contrário devolve `400`, igual a um código
/// inválido. Repetir com a conta já confirmada devolve `false` (idempotente).
pub async fn mark_email_verified(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
    expected_token_version: i32,
    expected_email_fingerprint: &str,
) -> Result<bool, AppError> {
    let user = repository::find_by_id(db, user_id)
        .await?
        .filter(|user| {
            user.active
                && user.token_version == expected_token_version
                && email_fingerprint(&user.email) == expected_email_fingerprint
        })
        .ok_or_else(|| AppError::BadRequest("código inválido ou expirado".to_owned()))?;

    Ok(repository::set_email_verified(db, user.id).await? > 0)
}

/// Confirma o e-mail manualmente (CLI), registrando a ação na auditoria.
///
/// `false` quando a conta já estava confirmada; repetir é idempotente.
pub async fn verify_email(
    db: &DatabaseConnection,
    actor: Actor<'_>,
    user_id: uuid::Uuid,
) -> Result<bool, AppError> {
    let txn = db.begin().await?;

    let user = repository::find_by_id(&txn, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("usuário não encontrado".to_owned()))?;

    if user.email_verified_at.is_some() {
        txn.rollback().await?;
        return Ok(false);
    }

    if repository::set_email_verified(&txn, user_id).await? == 0 {
        txn.rollback().await?;
        return Ok(false);
    }

    if let Err(error) = record_user(&txn, actor, Action::UserEmailVerified, user_id, None).await {
        txn.rollback().await?;
        return Err(error);
    }

    txn.commit().await?;
    Ok(true)
}

/// Backfill: confirma todos os usuários pendentes, um a um, com auditoria.
pub async fn verify_all_emails(db: &DatabaseConnection, actor: Actor<'_>) -> Result<u64, AppError> {
    let pending = repository::list_unverified(db).await?;
    let mut verified = 0;

    for user in pending {
        if verify_email(db, actor, user.id).await? {
            verified += 1;
        }
    }

    Ok(verified)
}
