pub mod m20261124_000001_create_users;
pub mod m20261124_000002_create_email_outbox;
pub mod m20261124_000003_add_users_role_index;

use sea_orm_migration::{MigrationTrait, prelude::*};

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261124_000001_create_users::Migration),
            Box::new(m20261124_000002_create_email_outbox::Migration),
            Box::new(m20261124_000003_add_users_role_index::Migration),
        ]
    }
}
