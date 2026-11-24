//! Cria a tabela `email_outbox` (padrão transactional outbox).

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EmailOutbox::Table)
                    .if_not_exists()
                    .col(pk_uuid(EmailOutbox::Id))
                    .col(string_len(EmailOutbox::ToEmail, 254))
                    .col(string_len(EmailOutbox::Subject, 200))
                    .col(text(EmailOutbox::Body))
                    .col(string_len(EmailOutbox::Status, 20).default(Expr::value("pending")))
                    .col(integer(EmailOutbox::Attempts).default(Expr::value(0)))
                    .col(
                        timestamp_with_time_zone(EmailOutbox::NextAttemptAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(timestamp_with_time_zone(EmailOutbox::LockedAt).null())
                    .col(text(EmailOutbox::LastError).null())
                    .col(string_len_uniq(EmailOutbox::DedupeKey, 200))
                    .col(timestamp_with_time_zone_default_now(EmailOutbox::CreatedAt))
                    .col(timestamp_with_time_zone(EmailOutbox::SentAt).null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("email_outbox_status_next_attempt_idx")
                    .table(EmailOutbox::Table)
                    .col(EmailOutbox::Status)
                    .col(EmailOutbox::NextAttemptAt)
                    .to_owned(),
            )
            .await?;

        // Postgres: estado só pode ser um dos conhecidos.
        manager
            .get_connection()
            .execute_raw(sea_orm::Statement::from_string(
                manager.get_database_backend(),
                "ALTER TABLE email_outbox ADD CONSTRAINT email_outbox_status_check \
                 CHECK (status IN ('pending', 'processing', 'sent', 'failed'))",
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EmailOutbox::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum EmailOutbox {
    Table,
    Id,
    ToEmail,
    Subject,
    Body,
    Status,
    Attempts,
    NextAttemptAt,
    LockedAt,
    LastError,
    DedupeKey,
    CreatedAt,
    SentAt,
}
