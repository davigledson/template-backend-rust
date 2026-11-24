//! Aplica as migrations pendentes. Roda antes da API, nunca no boot dela.

use sea_orm_migration::MigratorTrait;
use template_backend_rust::{config::Settings, database::connection, migrations::Migrator};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::load_unvalidated()?;
    let db = connection::connect(&settings.database).await?;

    let pending = Migrator::get_pending_migrations(&db).await?;
    if pending.is_empty() {
        println!("nenhuma migration pendente");
        return Ok(());
    }

    println!("aplicando {} migration(s)...", pending.len());
    Migrator::up(&db, None).await?;
    println!("migrations aplicadas");

    Ok(())
}
