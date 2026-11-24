//! Estado compartilhado da aplicação.
//!
//! Tudo o que os handlers precisam é clonável e barato (`Arc`/pools).

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::{
    config::settings::Settings,
    core::{auth::jwt::JwtService, cache::Cache},
};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub cache: Cache,
    pub jwt: JwtService,
    pub config: Arc<Settings>,
}

impl AppState {
    pub fn new(
        db: DatabaseConnection,
        cache: Cache,
        jwt: JwtService,
        config: Arc<Settings>,
    ) -> Self {
        Self {
            db,
            cache,
            jwt,
            config,
        }
    }
}
