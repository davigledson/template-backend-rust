//! Marcador de família revogada. Um reuso de refresh derruba a família inteira.

use std::time::Duration;

use uuid::Uuid;

use crate::{
    core::{cache::Cache, error::AppError},
    modules::auth::repository::ttl_secs,
};

const FAMILY_PREFIX: &str = "auth:refresh-family:";

pub async fn is_family_revoked(cache: &Cache, family_id: Uuid) -> Result<bool, AppError> {
    let mut connection = cache.pool().get().await?;
    let exists: i64 = redis::cmd("EXISTS")
        .arg(family_key(family_id))
        .query_async(&mut connection)
        .await?;
    Ok(exists == 1)
}

pub async fn revoke_family(cache: &Cache, family_id: Uuid, ttl: Duration) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let _: String = redis::cmd("SET")
        .arg(family_key(family_id))
        .arg("1")
        .arg("EX")
        .arg(ttl_secs(ttl))
        .query_async(&mut connection)
        .await?;
    Ok(())
}

pub(super) fn family_key(family_id: Uuid) -> String {
    format!("{FAMILY_PREFIX}{family_id}:revoked")
}
