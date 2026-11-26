//! Queries da tabela `email_outbox`. Só o módulo `email` escreve nela.

use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait, sea_query::Expr,
};

use crate::database::entity::email_outbox::{self, ActiveModel, Column, Entity};

/// Tempo que um item pode ficar em `processing` antes de outro worker assumir.
pub const LEASE_MINUTES: i64 = 5;

pub async fn enqueue<C>(conn: &C, model: ActiveModel) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    Entity::insert(model)
        .on_conflict_do_nothing_on([Column::DedupeKey])
        .exec_without_returning(conn)
        .await?;

    Ok(())
}

/// Reserva um lote com `FOR UPDATE SKIP LOCKED` e marca como `processing`.
///
/// O `commit` libera os locks; a entrega acontece fora da transação.
pub async fn claim_batch(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<email_outbox::Model>, DbErr> {
    let now = Utc::now().fixed_offset();
    let lease_cutoff = now - Duration::minutes(LEASE_MINUTES);

    let txn = db.begin().await?;

    let mut items = Entity::find()
        .filter(
            Condition::any()
                .add(
                    Condition::all()
                        .add(Column::Status.eq("pending"))
                        .add(Column::NextAttemptAt.lte(now)),
                )
                .add(
                    Condition::all()
                        .add(Column::Status.eq("processing"))
                        .add(Column::LockedAt.lt(lease_cutoff)),
                ),
        )
        .order_by_asc(Column::CreatedAt)
        .limit(limit)
        .lock_with_behavior(
            sea_orm::sea_query::LockType::Update,
            sea_orm::sea_query::LockBehavior::SkipLocked,
        )
        .all(&txn)
        .await?;

    for item in &mut items {
        Entity::update_many()
            .col_expr(Column::Status, Expr::value("processing"))
            .col_expr(Column::Attempts, Expr::col(Column::Attempts).add(1))
            .col_expr(Column::LockedAt, Expr::value(now))
            .filter(Column::Id.eq(item.id))
            .exec(&txn)
            .await?;

        item.status = "processing".to_owned();
        item.attempts += 1;
        item.locked_at = Some(now);
    }

    txn.commit().await?;
    Ok(items)
}

pub async fn mark_sent(db: &DatabaseConnection, id: uuid::Uuid) -> Result<u64, DbErr> {
    let now = Utc::now().fixed_offset();
    Entity::update_many()
        .col_expr(Column::Status, Expr::value("sent"))
        .col_expr(Column::SentAt, Expr::value(now))
        // O corpo pode conter códigos de recuperação/confirmação: depois do
        // envio só os metadados são úteis para inspeção e retenção.
        .col_expr(Column::Body, Expr::value(""))
        .col_expr(
            Column::LockedAt,
            Expr::value(Option::<DateTime<FixedOffset>>::None),
        )
        .col_expr(Column::LastError, Expr::value(Option::<String>::None))
        .filter(Column::Id.eq(id))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

pub async fn mark_retry(
    db: &DatabaseConnection,
    id: uuid::Uuid,
    next_attempt_at: DateTime<FixedOffset>,
    last_error: &str,
) -> Result<u64, DbErr> {
    Entity::update_many()
        .col_expr(Column::Status, Expr::value("pending"))
        .col_expr(Column::NextAttemptAt, Expr::value(next_attempt_at))
        .col_expr(
            Column::LockedAt,
            Expr::value(Option::<DateTime<FixedOffset>>::None),
        )
        .col_expr(Column::LastError, Expr::value(last_error))
        .filter(Column::Id.eq(id))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

pub async fn mark_failed(
    db: &DatabaseConnection,
    id: uuid::Uuid,
    last_error: &str,
) -> Result<u64, DbErr> {
    Entity::update_many()
        .col_expr(Column::Status, Expr::value("failed"))
        .col_expr(
            Column::LockedAt,
            Expr::value(Option::<DateTime<FixedOffset>>::None),
        )
        .col_expr(Column::LastError, Expr::value(last_error))
        .filter(Column::Id.eq(id))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

/// Remove e-mails enviados antigos (retenção).
pub async fn delete_sent_before(
    db: &DatabaseConnection,
    cutoff: DateTime<FixedOffset>,
) -> Result<u64, DbErr> {
    Entity::delete_many()
        .filter(Column::Status.eq("sent"))
        .filter(Column::SentAt.lt(cutoff))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

/// Contagem por status, para as métricas de profundidade do outbox.
pub async fn count_by_status(db: &DatabaseConnection, status: &str) -> Result<u64, DbErr> {
    Entity::find()
        .filter(Column::Status.eq(status))
        .count(db)
        .await
}

/// Enviados desde um instante, para a janela de 24 h.
pub async fn count_sent_since(
    db: &DatabaseConnection,
    cutoff: DateTime<FixedOffset>,
) -> Result<u64, DbErr> {
    Entity::find()
        .filter(Column::Status.eq("sent"))
        .filter(Column::SentAt.gte(cutoff))
        .count(db)
        .await
}

/// Itens de um status, mais antigos primeiro.
pub async fn list_by_status(
    db: &DatabaseConnection,
    status: &str,
    limit: u64,
) -> Result<Vec<email_outbox::Model>, DbErr> {
    Entity::find()
        .filter(Column::Status.eq(status))
        .order_by_asc(Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
}

/// Devolve um item ao processamento imediato, sem esperar o backoff.
///
/// Só aceita `pending`/`failed`: `sent` não deve ser reenviado e `processing`
/// pode estar em outra transação.
pub async fn requeue(db: &DatabaseConnection, id: uuid::Uuid) -> Result<u64, DbErr> {
    let now = Utc::now().fixed_offset();

    Entity::update_many()
        .col_expr(Column::Status, Expr::value("pending"))
        .col_expr(Column::NextAttemptAt, Expr::value(now))
        .col_expr(
            Column::LockedAt,
            Expr::value(Option::<DateTime<FixedOffset>>::None),
        )
        .filter(Column::Id.eq(id))
        .filter(Column::Status.is_in(["pending", "failed"]))
        .exec(db)
        .await
        .map(|result| result.rows_affected)
}

/// Reprocessa todos os itens de um status (`pending` ou `failed`).
pub async fn requeue_all(
    db: &DatabaseConnection,
    status: &str,
    reset_attempts: bool,
) -> Result<u64, DbErr> {
    let now = Utc::now().fixed_offset();

    let mut query = Entity::update_many()
        .col_expr(Column::Status, Expr::value("pending"))
        .col_expr(Column::NextAttemptAt, Expr::value(now))
        .col_expr(
            Column::LockedAt,
            Expr::value(Option::<DateTime<FixedOffset>>::None),
        )
        .filter(Column::Status.eq(status));

    if reset_attempts {
        query = query.col_expr(Column::Attempts, Expr::value(0));
    }

    query.exec(db).await.map(|result| result.rows_affected)
}
