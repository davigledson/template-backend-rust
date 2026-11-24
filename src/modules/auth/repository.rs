//! Refresh tokens opacos no Redis.
//!
//! - token aleatório (não JWT), guardado apenas como hash SHA-256;
//! - valor `user_id:token_version:family_id` permite invalidar todas as sessões
//!   quando a senha ou o papel muda (`token_version`) e revogar uma família
//!   inteira quando um token já consumido é reapresentado;
//! - a rotação é atômica e deixa um marcador de "já usado" com o mesmo TTL.

use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::core::{cache::Cache, error::AppError};

const PREFIX: &str = "auth:refresh:";
const USED_PREFIX: &str = "auth:refresh-used:";
const FAMILY_PREFIX: &str = "auth:refresh-family:";
const FALLBACK_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Consome o token e grava o marcador de uso na mesma operação atômica.
const CONSUME_SCRIPT: &str = r"
local value = redis.call('GET', KEYS[1])
if not value then return nil end
redis.call('DEL', KEYS[1])
redis.call('SET', KEYS[2], value, 'EX', ARGV[1])
return value
";

#[derive(Debug, Clone)]
pub struct RefreshTicket {
    pub user_id: Uuid,
    pub token_version: i32,
    pub family_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum ConsumeOutcome {
    /// Token válido; pode ser trocado por um par novo da mesma família.
    Active(RefreshTicket),
    /// Token já consumido; a família inteira deve ser revogada.
    Reused(Uuid),
    /// Token inexistente, expirado ou malformado.
    Unknown,
}

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
    let ttl_secs = ttl_secs(ttl);

    let _: String = redis::cmd("SET")
        .arg(key_for(token))
        .arg(value)
        .arg("EX")
        .arg(ttl_secs)
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

fn parse_ticket(value: &str) -> Option<RefreshTicket> {
    let mut parts = value.split(':');

    let user_id = Uuid::parse_str(parts.next()?).ok()?;
    let token_version = parts.next()?.parse::<i32>().ok()?;
    let family_id = Uuid::parse_str(parts.next()?).ok()?;

    Some(RefreshTicket {
        user_id,
        token_version,
        family_id,
    })
}

fn ttl_secs(ttl: Duration) -> i64 {
    i64::try_from(ttl.as_secs())
        .unwrap_or(FALLBACK_TTL_SECS)
        .max(1)
}

fn key_for(token: &str) -> String {
    format!("{PREFIX}{}", hash_token(token))
}

fn used_key_for(token: &str) -> String {
    format!("{USED_PREFIX}{}", hash_token(token))
}

fn family_key(family_id: Uuid) -> String {
    format!("{FAMILY_PREFIX}{family_id}:revoked")
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
    fn ticket_ida_e_volta() {
        let ticket = RefreshTicket {
            user_id: Uuid::now_v7(),
            token_version: 3,
            family_id: Uuid::now_v7(),
        };
        let value = format!(
            "{}:{}:{}",
            ticket.user_id, ticket.token_version, ticket.family_id
        );

        let parsed = parse_ticket(&value).expect("ticket");
        assert_eq!(parsed.user_id, ticket.user_id);
        assert_eq!(parsed.token_version, 3);
        assert_eq!(parsed.family_id, ticket.family_id);
    }

    #[test]
    fn formato_antigo_vira_unknown() {
        let value = format!("{}:0", Uuid::now_v7());
        assert!(parse_ticket(&value).is_none());
    }

    #[test]
    fn chaves_nao_guardam_token_em_claro() {
        let token = generate_token();
        assert!(!key_for(&token).contains(&token));
        assert!(!used_key_for(&token).contains(&token));
    }
}
