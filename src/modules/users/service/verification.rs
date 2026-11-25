//! Confirmação de e-mail: emissão do token e marcação de verificado.
//!
//! Usada por `users` (criação/troca de e-mail) e por `auth` (confirmação e
//! reenvio), sempre através desta porta.

use sea_orm::DatabaseConnection;

use crate::{
    core::{auth::verification_token, cache::Cache, error::AppError, mail::mask_email},
    database::entity::users,
    modules::{
        email::{service as email_service, templates as email_templates},
        users::repository,
    },
};

/// Envia (ou reenvia) o código de confirmação. Best-effort: falha de e-mail ou
/// Redis não pode impedir a criação do usuário.
pub async fn send_email_verification(db: &DatabaseConnection, cache: &Cache, user: &users::Model) {
    let token = match verification_token::issue(cache, user.id).await {
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

/// Marca o e-mail como confirmado. Idempotente: repetir não altera a data.
pub async fn mark_email_verified(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<bool, AppError> {
    Ok(repository::set_email_verified(db, user_id).await? > 0)
}
