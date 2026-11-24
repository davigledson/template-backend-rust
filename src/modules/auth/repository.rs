//! Refresh tokens opacos no Redis.
//!
//! - token aleatório (não JWT), guardado apenas como hash SHA-256;
//! - valor `user_id:token_version` permite invalidar todas as sessões
//!   quando a senha ou o papel muda (basta incrementar `token_version`);
//! - `GETDEL` garante rotação atômica.

use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::core::{cache::Cache, error::AppError};

const PREFIX: &str = "auth:refresh:";
const FALLBACK_TTL_SECS: i64 = 7 * 24 * 60 * 60;

pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub async fn store(
    cache: &Cache,
    token: &str,
    user_id: Uuid,
    token_version: i32,
    ttl: Duration,
) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let value = format!("{user_id}:{token_version}");
    let ttl_secs = i64::try_from(ttl.as_secs())
        .unwrap_or(FALLBACK_TTL_SECS)
        .max(1);

    let _: String = redis::cmd("SET")
        .arg(key_for(token))
        .arg(value)
        .arg("EX")
        .arg(ttl_secs)
        .query_async(&mut connection)
        .await?;

    Ok(())
}

pub async fn consume(cache: &Cache, token: &str) -> Result<Option<(Uuid, i32)>, AppError> {
    let mut connection = cache.pool().get().await?;

    let value: Option<String> = redis::cmd("GETDEL")
        .arg(key_for(token))
        .query_async(&mut connection)
        .await?;

    let Some(value) = value else {
        return Ok(None);
    };

    let Some((id, version)) = value.split_once(':') else {
        return Ok(None);
    };

    let Ok(user_id) = Uuid::parse_str(id) else {
        return Ok(None);
    };

    let Ok(token_version) = version.parse::<i32>() else {
        return Ok(None);
    };

    Ok(Some((user_id, token_version)))
}

pub async fn revoke(cache: &Cache, token: &str) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let _: i64 = redis::cmd("DEL")
        .arg(key_for(token))
        .query_async(&mut connection)
        .await?;
    Ok(())
}

fn key_for(token: &str) -> String {
    format!("{PREFIX}{}", hash_token(token))
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
