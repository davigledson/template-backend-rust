//! Índice para o filtro `GET /users?role=...&active=...`.
//!
//! A listagem filtra papel e situação juntos; sem este índice o Postgres varre
//! a tabela inteira assim que ela cresce.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("users_role_active_idx")
                    .table(Users::Table)
                    .col(Users::Role)
                    .col(Users::Active)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("users_role_active_idx")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Role,
    Active,
}
