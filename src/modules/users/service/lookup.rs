//! Leituras de `users` usadas dentro e fora do módulo.

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::{
    core::error::AppError,
    database::entity::users,
    modules::users::{dto::ListUsersQuery, repository},
};

pub use super::rules::normalize_email;

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
