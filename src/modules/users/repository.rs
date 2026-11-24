//! Queries da tabela `users`. Só este módulo escreve nela.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, sea_query::Expr,
};
use uuid::Uuid;

use crate::{
    core::auth::role::Role,
    database::entity::users::{self, ActiveModel, Column, Entity},
};

pub async fn find_by_id(db: &DatabaseConnection, id: Uuid) -> Result<Option<users::Model>, DbErr> {
    Entity::find_by_id(id).one(db).await
}

pub async fn find_by_email(
    db: &DatabaseConnection,
    email: &str,
) -> Result<Option<users::Model>, DbErr> {
    Entity::find().filter(Column::Email.eq(email)).one(db).await
}

pub async fn insert(db: &DatabaseConnection, user: ActiveModel) -> Result<users::Model, DbErr> {
    user.insert(db).await
}

pub async fn count_active_admins(db: &DatabaseConnection) -> Result<u64, DbErr> {
    Entity::find()
        .filter(Column::Role.eq(Role::Admin.as_str()))
        .filter(Column::Active.eq(true))
        .count(db)
        .await
}

/// Atualiza papel e invalida sessões (token_version).
pub async fn set_role(db: &DatabaseConnection, id: Uuid, role: Role) -> Result<u64, DbErr> {
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

/// Atualiza a senha e invalida sessões (token_version).
pub async fn set_password(
    db: &DatabaseConnection,
    id: Uuid,
    password_hash: &str,
) -> Result<u64, DbErr> {
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
