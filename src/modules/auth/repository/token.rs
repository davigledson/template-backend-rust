//! Ciclo do refresh token: emitir, consumir (com rotação) e revogar.

use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    core::{cache::Cache, error::AppError},
    modules::auth::repository::{ConsumeOutcome, ttl_secs},
};

use super::ticket::parse_ticket;

const PREFIX: &str = "auth:refresh:";
const USED_PREFIX: &str = "auth:refresh-used:";

/// Consome o token e grava o marcador de uso na mesma operação atômica.
const CONSUME_SCRIPT: &str = r"
local value = redis.call('GET', KEYS[1])
if not value then return nil end
redis.call('DEL', KEYS[1])
redis.call('SET', KEYS[2], value, 'EX', ARGV[1])
return value
";

pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn new_family_id() -> Uuid {
    Uuid::now_v7()
}

pub async fn store(
    cache: &Cache,
    token: &str,
    user_id: Uuid,
    token_version: i32,
    family_id: Uuid,
    ttl: Duration,
) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let value = format!("{user_id}:{token_version}:{family_id}");

    let _: String = redis::cmd("SET")
        .arg(key_for(token))
        .arg(value)
        .arg("EX")
        .arg(ttl_secs(ttl))
        .query_async(&mut connection)
        .await?;

    Ok(())
}

pub async fn consume(
    cache: &Cache,
    token: &str,
    ttl: Duration,
) -> Result<ConsumeOutcome, AppError> {
    let mut connection = cache.pool().get().await?;

    let value: Option<String> = redis::Script::new(CONSUME_SCRIPT)
        .key(key_for(token))
        .key(used_key_for(token))
        .arg(ttl_secs(ttl))
        .invoke_async(&mut connection)
        .await?;

    if let Some(value) = value {
        return Ok(match parse_ticket(&value) {
            Some(ticket) => ConsumeOutcome::Active(ticket),
            None => ConsumeOutcome::Unknown,
        });
    }

    let used: Option<String> = redis::cmd("GET")
        .arg(used_key_for(token))
        .query_async(&mut connection)
        .await?;

    Ok(match used.as_deref().and_then(parse_ticket) {
        Some(ticket) => ConsumeOutcome::Reused(ticket.family_id),
        None => ConsumeOutcome::Unknown,
    })
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

fn used_key_for(token: &str) -> String {
    format!("{USED_PREFIX}{}", hash_token(token))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaves_nao_guardam_token_em_claro() {
        let token = generate_token();
        assert!(!key_for(&token).contains(&token));
        assert!(!used_key_for(&token).contains(&token));
    }
}
