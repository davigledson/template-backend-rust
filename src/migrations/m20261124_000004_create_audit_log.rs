//! Cria a tabela `audit_log` (registro append-only de ações privilegiadas).
//!
//! `actor_id` é nulo quando a ação veio da CLI ou de um job. Não há FK de
//! propósito: a auditoria é um retrato histórico e não pode impedir a remoção
//! de um usuário nem mudar junto com ele.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLog::Table)
                    .if_not_exists()
                    .col(pk_uuid(AuditLog::Id))
                    .col(uuid(AuditLog::ActorId).null())
                    .col(string_len(AuditLog::ActorRole, 20).null())
                    .col(string_len(AuditLog::Action, 40))
                    .col(string_len(AuditLog::TargetType, 30))
                    .col(uuid(AuditLog::TargetId))
                    .col(json(AuditLog::Changes).null())
                    .col(string_len(AuditLog::RequestId, 64).null())
                    .col(timestamp_with_time_zone_default_now(AuditLog::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("audit_log_target_created_idx")
                    .table(AuditLog::Table)
                    .col(AuditLog::TargetId)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("audit_log_actor_created_idx")
                    .table(AuditLog::Table)
                    .col(AuditLog::ActorId)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("audit_log_created_idx")
                    .table(AuditLog::Table)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLog::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Id,
    ActorId,
    ActorRole,
    Action,
    TargetType,
    TargetId,
    Changes,
    RequestId,
    CreatedAt,
}
