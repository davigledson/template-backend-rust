//! Conexão com o PostgreSQL via pool do SeaORM.

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::config::settings::DatabaseSettings;

/// Conexão sem `statement_timeout`; usada por worker, migrator e CLI.
pub async fn connect(settings: &DatabaseSettings) -> Result<DatabaseConnection, DbErr> {
    connect_inner(settings, false).await
}

/// Conexão da API: aplica `database.statement_timeout_secs` no próprio
/// PostgreSQL (startup option, sem round-trip por query).
pub async fn connect_api(settings: &DatabaseSettings) -> Result<DatabaseConnection, DbErr> {
    connect_inner(settings, true).await
}

async fn connect_inner(
    settings: &DatabaseSettings,
    apply_statement_timeout: bool,
) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(settings.url.clone());
    options
        .max_connections(settings.max_connections)
        .min_connections(settings.min_connections)
        .acquire_timeout(settings.acquire_timeout())
        .connect_timeout(settings.connect_timeout())
        .sqlx_logging(settings.sqlx_logging);

    if apply_statement_timeout && let Some(timeout) = settings.statement_timeout() {
        let value = format!("{}s", timeout.as_secs());
        options.map_sqlx_postgres_opts(move |opts| {
            opts.options([("statement_timeout", value.clone())])
        });
    }

    Database::connect(options).await
}
