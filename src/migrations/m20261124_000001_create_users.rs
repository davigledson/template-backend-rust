//! Cria a tabela `users` (auth + módulo-referência).

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(pk_uuid(Users::Id))
                    .col(string_len(Users::Name, 120))
                    .col(string_len_uniq(Users::Email, 254))
                    .col(string_len(Users::PasswordHash, 255))
                    .col(string_len(Users::Role, 20).default(Expr::value("user")))
                    .col(boolean(Users::Active).default(Expr::value(true)))
                    .col(integer(Users::TokenVersion).default(Expr::value(0)))
                    .col(integer(Users::Version).default(Expr::value(0)))
                    .col(timestamp_with_time_zone_default_now(Users::CreatedAt))
                    .col(timestamp_with_time_zone_default_now(Users::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("users_active_created_idx")
                    .table(Users::Table)
                    .col(Users::Active)
                    .col(Users::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Postgres: papel só pode ser admin ou user.
        manager
            .get_connection()
            .execute_raw(sea_orm::Statement::from_string(
                manager.get_database_backend(),
                "ALTER TABLE users ADD CONSTRAINT users_role_check \
                 CHECK (role IN ('admin', 'user'))",
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Name,
    Email,
    PasswordHash,
    Role,
    Active,
    TokenVersion,
    Version,
    CreatedAt,
    UpdatedAt,
}
