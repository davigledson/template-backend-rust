//! Leitura: busca por id/e-mail, listagem paginada e contagens.

use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};
use uuid::Uuid;

use crate::{
    core::auth::role::Role,
    database::entity::users::{self, Column, Entity},
};

pub async fn find_by_id<C>(db: &C, id: Uuid) -> Result<Option<users::Model>, DbErr>
where
    C: ConnectionTrait,
{
    Entity::find_by_id(id).one(db).await
}

pub async fn find_by_email<C>(db: &C, email: &str) -> Result<Option<users::Model>, DbErr>
where
    C: ConnectionTrait,
{
    Entity::find().filter(Column::Email.eq(email)).one(db).await
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

pub async fn count_by_email<C>(db: &C, email: &str, excluding: Uuid) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    Entity::find()
        .filter(Column::Email.eq(email))
        .filter(Column::Id.ne(excluding))
        .count(db)
        .await
}

/// Escapa curingas do `LIKE`/`ILIKE` e envolve o termo em `%`.
fn like_pattern(term: &str) -> String {
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
