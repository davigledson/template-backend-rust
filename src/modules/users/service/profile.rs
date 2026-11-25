//! O próprio usuário: nome do perfil e sessão corrente.

use sea_orm::DatabaseConnection;

use crate::{
    core::{auth::extractor::AuthUser, error::AppError},
    database::entity::users,
    modules::users::{repository, service::session::require_session},
};

/// Atualiza o próprio nome. O papel e o e-mail não mudam por aqui.
pub async fn update_profile(
    db: &DatabaseConnection,
    auth: &AuthUser,
    name: &str,
) -> Result<users::Model, AppError> {
    let current = require_session(db, auth).await?;
    let name = name.trim();

    if name.is_empty() {
        return Err(AppError::BadRequest("nome é obrigatório".to_owned()));
    }

    let updated = repository::update(
        db,
        current.id,
        current.version,
        Some(name.to_owned()),
        None,
        None,
        None,
        false,
        false,
    )
    .await?;

    if updated == 0 {
        return Err(AppError::Conflict(
            "perfil foi alterado por outra sessão; recarregue e tente novamente".to_owned(),
        ));
    }

    repository::find_by_id(db, current.id)
        .await?
        .ok_or_else(|| AppError::internal("usuário desapareceu após a atualização"))
}
