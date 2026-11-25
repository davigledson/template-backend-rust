//! Metadados de sessão por dispositivo: uma entrada por família de refresh.
//!
//! Ficam no Redis com o mesmo TTL do refresh; se o Redis reiniciar, as sessões
//! caem junto (comportamento esperado e documentado).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{cache::Cache, error::AppError};

const PREFIX: &str = "auth:session:";
const MAX_USER_AGENT: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub family_id: Uuid,
    /// Epoch em milissegundos.
    pub created_at: i64,
    /// Epoch em milissegundos.
    pub last_seen_at: i64,
    pub ip: String,
    pub user_agent: Option<String>,
}

/// Registra o login. Repetir com a mesma família preserva `created_at`.
pub async fn register(
    cache: &Cache,
    user_id: Uuid,
    family_id: Uuid,
    ttl: Duration,
    ip: &str,
    user_agent: Option<&str>,
) -> Result<(), AppError> {
    let now = now_ms();
    let record = SessionRecord {
        family_id,
        created_at: now,
        last_seen_at: now,
        ip: ip.to_owned(),
        user_agent: user_agent.map(truncate_user_agent),
    };

    store(cache, user_id, &record, ttl).await
}

/// Atualiza `last_seen_at` e renova o TTL. Ignora sessão desconhecida.
pub async fn touch(
    cache: &Cache,
    user_id: Uuid,
    family_id: Uuid,
    ttl: Duration,
) -> Result<(), AppError> {
    let Some(mut record) = find(cache, user_id, family_id).await? else {
        return Ok(());
    };

    record.last_seen_at = now_ms();
    store(cache, user_id, &record, ttl).await
}

pub async fn list(cache: &Cache, user_id: Uuid) -> Result<Vec<SessionRecord>, AppError> {
    let mut connection = cache.pool().get().await?;
    let pattern = pattern_for(user_id);
    let mut cursor: u64 = 0;
    let mut sessions = Vec::new();

    loop {
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut connection)
            .await?;

        for key in keys {
            let value: Option<String> = redis::cmd("GET")
                .arg(&key)
                .query_async(&mut connection)
                .await?;

            if let Some(record) = value.as_deref().and_then(parse) {
                sessions.push(record);
            }
        }

        cursor = next;
        if cursor == 0 {
            break;
        }
    }

    sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
    Ok(sessions)
}

pub async fn remove(cache: &Cache, user_id: Uuid, family_id: Uuid) -> Result<bool, AppError> {
    let mut connection = cache.pool().get().await?;
    let removed: i64 = redis::cmd("DEL")
        .arg(key_for(user_id, family_id))
        .query_async(&mut connection)
        .await?;

    Ok(removed > 0)
}

/// Remove todas as sessões do usuário e devolve as famílias apagadas.
pub async fn clear(cache: &Cache, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
    let sessions = list(cache, user_id).await?;
    let mut removed = Vec::with_capacity(sessions.len());

    for session in sessions {
        if remove(cache, user_id, session.family_id).await? {
            removed.push(session.family_id);
        }
    }

    Ok(removed)
}

async fn find(
    cache: &Cache,
    user_id: Uuid,
    family_id: Uuid,
) -> Result<Option<SessionRecord>, AppError> {
    let mut connection = cache.pool().get().await?;
    let value: Option<String> = redis::cmd("GET")
        .arg(key_for(user_id, family_id))
        .query_async(&mut connection)
        .await?;

    Ok(value.as_deref().and_then(parse))
}

async fn store(
    cache: &Cache,
    user_id: Uuid,
    record: &SessionRecord,
    ttl: Duration,
) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let value = serde_json::to_string(record).map_err(AppError::internal)?;

    let _: String = redis::cmd("SET")
        .arg(key_for(user_id, record.family_id))
        .arg(value)
        .arg("EX")
        .arg(
            i64::try_from(ttl.as_secs())
                .unwrap_or(7 * 24 * 60 * 60)
                .max(1),
        )
        .query_async(&mut connection)
        .await?;

    Ok(())
}

fn key_for(user_id: Uuid, family_id: Uuid) -> String {
    format!("{PREFIX}{user_id}:{family_id}")
}

fn pattern_for(user_id: Uuid) -> String {
    format!("{PREFIX}{user_id}:*")
}

fn parse(value: &str) -> Option<SessionRecord> {
    serde_json::from_str(value).ok()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn truncate_user_agent(value: &str) -> String {
    value.chars().take(MAX_USER_AGENT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaves_sao_por_usuario_e_familia() {
        let user = Uuid::now_v7();
        let family = Uuid::now_v7();

        assert!(key_for(user, family).starts_with(&format!("{PREFIX}{user}:")));
        assert_eq!(pattern_for(user), format!("{PREFIX}{user}:*"));
    }

    #[test]
    fn user_agent_e_limitado() {
        let long = "a".repeat(MAX_USER_AGENT + 50);
        assert_eq!(truncate_user_agent(&long).chars().count(), MAX_USER_AGENT);
    }
}
