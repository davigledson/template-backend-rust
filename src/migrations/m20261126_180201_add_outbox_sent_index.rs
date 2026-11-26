//! Índice para expurgo e métricas de e-mails enviados.
//!
//! `purge_sent` e `count_sent_since` filtram `status = 'sent'` e `sent_at`; o
//! índice de claim do outbox não cobre essas consultas.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("email_outbox_status_sent_idx")
                    .table(EmailOutbox::Table)
                    .col(EmailOutbox::Status)
                    .col(EmailOutbox::SentAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("email_outbox_status_sent_idx")
                    .table(EmailOutbox::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum EmailOutbox {
    Table,
    Status,
    SentAt,
}
