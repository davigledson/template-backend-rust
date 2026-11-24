//! Sessão: o token ainda representa o estado atual do usuário?

use sea_orm::DatabaseConnection;

use crate::{
    core::{auth::extractor::AuthUser, error::AppError},
    database::entity::users,
    modules::users::repository,
};

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

pub async fn count_active_admins(db: &DatabaseConnection) -> Result<u64, AppError> {
    Ok(repository::count_active_admins(db).await?)
}
