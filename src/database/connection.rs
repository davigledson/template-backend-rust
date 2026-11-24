//! Conexão com o PostgreSQL via pool do SeaORM.

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::config::settings::DatabaseSettings;

pub async fn connect(settings: &DatabaseSettings) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(settings.url.clone());
    options
        .max_connections(settings.max_connections)
        .min_connections(settings.min_connections)
        .acquire_timeout(settings.acquire_timeout())
        .connect_timeout(settings.connect_timeout())
        .sqlx_logging(settings.sqlx_logging);

    Database::connect(options).await
}
