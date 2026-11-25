//! Escrita: insert e updates com concorrência otimista.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, ExprTrait, QueryFilter,
    sea_query::Expr,
};
use uuid::Uuid;

use crate::{
    core::auth::role::Role,
    database::entity::users::{self, ActiveModel, Column, Entity},
};

pub async fn insert<C>(db: &C, user: ActiveModel) -> Result<users::Model, DbErr>
where
    C: ConnectionTrait,
{
    user.insert(db).await
}

/// Atualiza apenas os campos presentes e incrementa `version`.
///
/// O filtro por `version` evita sobrescrever uma alteração concorrente.
/// `bump_token_version` revoga as sessões do usuário.
#[allow(clippy::too_many_arguments)]
pub async fn update<C>(
    db: &C,
    id: Uuid,
    expected_version: i32,
    name: Option<String>,
    email: Option<String>,
    role: Option<Role>,
    active: Option<bool>,
    bump_token_version: bool,
) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    let mut query = Entity::update_many()
        .col_expr(Column::UpdatedAt, Expr::current_timestamp())
        .col_expr(Column::Version, Expr::col(Column::Version).add(1))
        .filter(Column::Id.eq(id))
        .filter(Column::Version.eq(expected_version));

    if let Some(name) = name {
        query = query.col_expr(Column::Name, Expr::value(name));
    }
    if let Some(email) = email {
        query = query.col_expr(Column::Email, Expr::value(email));
    }
    if let Some(role) = role {
        query = query.col_expr(Column::Role, Expr::value(role.as_str()));
    }
    if let Some(active) = active {
        query = query.col_expr(Column::Active, Expr::value(active));
    }
    if bump_token_version {
        query = query.col_expr(Column::TokenVersion, Expr::col(Column::TokenVersion).add(1));
    }

    query.exec(db).await.map(|result| result.rows_affected)
}

/// Atualiza papel e invalida sessões (`token_version`).
pub async fn set_role<C>(db: &C, id: Uuid, role: Role) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    Entity::update_many()
        .col_expr(Column::Role, Expr::value(role.as_str()))
        .col_expr(Column::TokenVersion, Expr::col(Column::TokenVersion).add(1))
        .col_expr(Column::Version, Expr::col(Column::Version).add(1))
        .col_expr(Column::UpdatedAt, Expr::current_timestamp())
        .filter(Column::Id.eq(id))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

/// Atualiza a senha e invalida sessões (`token_version`).
pub async fn set_password<C>(db: &C, id: Uuid, password_hash: &str) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    Entity::update_many()
        .col_expr(Column::PasswordHash, Expr::value(password_hash))
        .col_expr(Column::TokenVersion, Expr::col(Column::TokenVersion).add(1))
        .col_expr(Column::Version, Expr::col(Column::Version).add(1))
        .col_expr(Column::UpdatedAt, Expr::current_timestamp())
        .filter(Column::Id.eq(id))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}
