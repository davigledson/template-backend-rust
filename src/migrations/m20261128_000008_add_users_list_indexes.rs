//! Índices para a listagem de usuários ordenada por criação.
//!
//! A listagem padrão ordena por `created_at DESC, id DESC`; sem um índice nessa
//! ordem o Postgres ordena a tabela inteira. O índice composto de papel/situação
//! também cobre o antigo `users_role_active_idx`, que passa a ser redundante.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("users_created_id_idx")
                    .table(Users::Table)
                    .col((Users::CreatedAt, IndexOrder::Desc))
                    .col((Users::Id, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("users_role_active_created_idx")
                    .table(Users::Table)
                    .col(Users::Role)
                    .col(Users::Active)
                    .col((Users::CreatedAt, IndexOrder::Desc))
                    .col((Users::Id, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // O novo índice começa com (role, active), então cobre as mesmas
        // consultas do índice antigo; manter os dois só encarece a escrita.
        manager
            .drop_index(
                Index::drop()
                    .name("users_role_active_idx")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("users_role_active_idx")
                    .table(Users::Table)
                    .col(Users::Role)
                    .col(Users::Active)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("users_role_active_created_idx")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("users_created_id_idx")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Role,
    Active,
    CreatedAt,
}
