//! Queries da tabela `audit_log`. Só este módulo escreve nela.
//!
//! A aplicação só faz insert e consulta. O único delete é o expurgo por
//! retenção, chamado pelo job diário do worker.

use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use crate::database::entity::audit_log::{self, ActiveModel, Column, Entity};

/// Filtros da consulta de auditoria.
#[derive(Debug, Default, Clone)]
pub struct Filter<'a> {
    pub actor_id: Option<Uuid>,
    pub target_id: Option<Uuid>,
    pub action: Option<&'a str>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn insert<C>(db: &C, entry: ActiveModel) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    Entity::insert(entry).exec_without_returning(db).await?;
    Ok(())
}

/// Remove registros mais antigos que `cutoff` (retenção).
pub async fn delete_before(
    db: &DatabaseConnection,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> Result<u64, DbErr> {
    Entity::delete_many()
        .filter(Column::CreatedAt.lt(cutoff.fixed_offset()))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

pub async fn list(
    db: &DatabaseConnection,
    offset: u64,
    limit: u64,
    filter: &Filter<'_>,
) -> Result<(Vec<audit_log::Model>, u64), DbErr> {
    let mut condition = Condition::all();

    if let Some(actor_id) = filter.actor_id {
        condition = condition.add(Column::ActorId.eq(actor_id));
    }
    if let Some(target_id) = filter.target_id {
        condition = condition.add(Column::TargetId.eq(target_id));
    }
    if let Some(action) = filter.action {
        condition = condition.add(Column::Action.eq(action));
    }
    if let Some(from) = filter.from {
        condition = condition.add(Column::CreatedAt.gte(from.fixed_offset()));
    }
    if let Some(to) = filter.to {
        condition = condition.add(Column::CreatedAt.lte(to.fixed_offset()));
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
