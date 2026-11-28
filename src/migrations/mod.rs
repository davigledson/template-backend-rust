pub mod m20261124_000001_create_users;
pub mod m20261124_000002_create_email_outbox;
pub mod m20261124_000003_add_users_role_index;
pub mod m20261124_000004_create_audit_log;
pub mod m20261124_000005_add_email_verified_at;
pub mod m20261126_000006_add_audit_action_index;
pub mod m20261126_180201_add_outbox_sent_index;
pub mod m20261128_000008_add_users_list_indexes;

use sea_orm_migration::{MigrationTrait, prelude::*};

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261124_000001_create_users::Migration),
            Box::new(m20261124_000002_create_email_outbox::Migration),
            Box::new(m20261124_000003_add_users_role_index::Migration),
            Box::new(m20261124_000004_create_audit_log::Migration),
            Box::new(m20261124_000005_add_email_verified_at::Migration),
            Box::new(m20261126_000006_add_audit_action_index::Migration),
            Box::new(m20261126_180201_add_outbox_sent_index::Migration),
            Box::new(m20261128_000008_add_users_list_indexes::Migration),
        ]
    }
}
