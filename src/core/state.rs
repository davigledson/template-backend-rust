//! Estado compartilhado da aplicação.
//!
//! Tudo o que os handlers precisam é clonável e barato (`Arc`/pools).

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::{
    config::settings::Settings,
    core::{auth::jwt::JwtService, cache::Cache, mail::Mailer},
};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub cache: Cache,
    pub jwt: JwtService,
    pub mailer: Arc<dyn Mailer>,
    pub config: Arc<Settings>,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        cache: Cache,
        jwt: JwtService,
        mailer: Arc<dyn Mailer>,
        config: Arc<Settings>,
    ) -> Self {
        Self {
            db,
            cache,
            jwt,
            mailer,
            config,
        }
    }
}
