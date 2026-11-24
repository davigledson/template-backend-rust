//! Queries da tabela `users`. Só este módulo escreve nela.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
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

#[allow(clippy::too_many_arguments)]
pub async fn list(
    db: &DatabaseConnection,
    offset: u64,
    limit: u64,
    search: Option<&str>,
    role: Option<Role>,
    active: Option<bool>,
) -> Result<(Vec<users::Model>, u64), DbErr> {
    let mut condition = Condition::all();

    if let Some(term) = search.map(str::trim).filter(|term| !term.is_empty()) {
        condition = condition.add(Expr::cust_with_values(
            "(name ILIKE $1 ESCAPE '\\' OR email ILIKE $1 ESCAPE '\\')",
            [like_pattern(term)],
        ));
    }
    if let Some(role) = role {
        condition = condition.add(Column::Role.eq(role.as_str()));
    }
    if let Some(active) = active {
        condition = condition.add(Column::Active.eq(active));
    }

    let base = Entity::find().filter(condition);
    let total = base.clone().count(db).await?;
    let items = base
        .order_by_desc(Column::CreatedAt)
        .order_by_desc(Column::Id)
        .offset(offset)
        .limit(limit)
        .all(db)
        .await?;

    Ok((items, total))
}

pub async fn count_active_admins(db: &DatabaseConnection) -> Result<u64, DbErr> {
    Entity::find()
        .filter(Column::Role.eq(Role::Admin.as_str()))
        .filter(Column::Active.eq(true))
        .count(db)
        .await
}

/// Atualiza apenas os campos presentes e incrementa `version`.
///
/// O filtro por `version` evita sobrescrever uma alteração concorrente.
/// `bump_token_version` revoga as sessões do usuário.
#[allow(clippy::too_many_arguments)]
pub async fn update(
    db: &DatabaseConnection,
    id: Uuid,
    expected_version: i32,
    name: Option<String>,
    email: Option<String>,
    role: Option<Role>,
    active: Option<bool>,
    bump_token_version: bool,
) -> Result<u64, DbErr> {
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

pub async fn count_by_email(
    db: &DatabaseConnection,
    email: &str,
    excluding: Uuid,
) -> Result<u64, DbErr> {
    Entity::find()
        .filter(Column::Email.eq(email))
        .filter(Column::Id.ne(excluding))
        .count(db)
        .await
}

/// Atualiza papel e invalida sessões (`token_version`).
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

/// Atualiza a senha e invalida sessões (`token_version`).
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

/// Escapa curingas do `LIKE`/`ILIKE` e envolve o termo em `%`.
pub fn like_pattern(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 2);
    escaped.push('%');
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('%');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapa_curingas_da_busca() {
        assert_eq!(like_pattern("ana"), "%ana%");
        assert_eq!(like_pattern("100%"), "%100\\%%");
        assert_eq!(like_pattern("a_b"), "%a\\_b%");
        assert_eq!(like_pattern("c\\d"), "%c\\\\d%");
    }
}
