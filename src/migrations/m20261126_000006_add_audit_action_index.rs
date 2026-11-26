//! Índice para o filtro `GET /audit?action=...`.
//!
//! A consulta já ordena por `created_at`; o índice composto evita varrer a
//! tabela quando a auditoria cresce.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("audit_log_action_created_idx")
                    .table(AuditLog::Table)
                    .col(AuditLog::Action)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("audit_log_action_created_idx")
                    .table(AuditLog::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Action,
    CreatedAt,
}
