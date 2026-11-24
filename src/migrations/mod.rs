pub mod m20261124_000001_create_users;

use sea_orm_migration::{MigrationTrait, prelude::*};

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261124_000001_create_users::Migration)]
    }
}
