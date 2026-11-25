//! Rate limit de login e de recuperação de senha com contadores no Redis.
//!
//! Janela fixa: a primeira falha cria a chave com TTL; cada falha incrementa.
//! O bloqueio é verificado **por conta** (e-mail normalizado, hasheado) e por
//! IP de origem, sem gravar e-mail em claro nas chaves.
//!
//! Se o Redis estiver indisponível, o login segue (fail-open) com log de erro:
//! indisponibilidade de cache não pode derrubar a autenticação inteira. O
//! proxy ainda pode aplicar um limite por IP como segunda barreira.

use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::core::cache::{Cache, CacheError};

const LOGIN_ACCOUNT_PREFIX: &str = "auth:login:fail:account:";
const LOGIN_IP_PREFIX: &str = "auth:login:fail:ip:";
const RESET_ACCOUNT_PREFIX: &str = "auth:pwreset:fail:account:";
const RESET_IP_PREFIX: &str = "auth:pwreset:fail:ip:";
const VERIFY_PREFIX: &str = "auth:verify:fail:user:";

/// Incrementa e define o TTL apenas na primeira falha da janela.
const FAILURE_SCRIPT: &str = r"
local count = redis.call('INCR', KEYS[1])
if count == 1 then
  redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return count
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitDecision {
    Allowed,
    Blocked { retry_after_secs: u64 },
}

pub struct RateLimiter<'a> {
    cache: &'a Cache,
    window: Duration,
}

impl<'a> RateLimiter<'a> {
    pub fn new(cache: &'a Cache, window: Duration) -> Self {
        Self { cache, window }
    }

    pub fn account_key(normalized_email: &str) -> String {
        format!("{LOGIN_ACCOUNT_PREFIX}{}", hash_subject(normalized_email))
    }

    pub fn ip_key(ip: &str) -> String {
        format!("{LOGIN_IP_PREFIX}{}", hash_subject(ip))
    }

    /// Chave de pedidos de recuperação de senha por conta.
    pub fn reset_account_key(normalized_email: &str) -> String {
        format!("{RESET_ACCOUNT_PREFIX}{}", hash_subject(normalized_email))
    }

    /// Chave de pedidos de recuperação de senha por IP.
    pub fn reset_ip_key(ip: &str) -> String {
        format!("{RESET_IP_PREFIX}{}", hash_subject(ip))
    }

    /// Chave de reenvios de confirmação de e-mail por usuário.
    pub fn verification_key(user_id: uuid::Uuid) -> String {
        format!("{VERIFY_PREFIX}{user_id}")
    }

    pub async fn check(&self, key: &str, max_attempts: u32) -> Result<LimitDecision, CacheError> {
        let mut connection = self.cache.pool().get().await?;
        let count: Option<u32> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut connection)
            .await?;

        if count.unwrap_or(0) < max_attempts {
            return Ok(LimitDecision::Allowed);
        }

        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(&mut connection)
            .await?;

        Ok(LimitDecision::Blocked {
            retry_after_secs: u64::try_from(ttl).unwrap_or(1).max(1),
        })
    }

    pub async fn register_failure(&self, key: &str) -> Result<(), CacheError> {
        let mut connection = self.cache.pool().get().await?;
        let ttl = i64::try_from(self.window.as_secs()).unwrap_or(900).max(1);

        let _: i64 = redis::Script::new(FAILURE_SCRIPT)
            .key(key)
            .arg(ttl)
            .invoke_async(&mut connection)
            .await?;

        Ok(())
    }

    pub async fn clear(&self, key: &str) -> Result<(), CacheError> {
        let mut connection = self.cache.pool().get().await?;
        let _: i64 = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut connection)
            .await?;
        Ok(())
    }
}

fn hash_subject(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
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
    fn chave_de_conta_nao_contem_email() {
        let key = RateLimiter::account_key("ana@teste.local");
        assert!(key.starts_with(LOGIN_ACCOUNT_PREFIX));
        assert!(!key.contains("ana"));
        assert!(!key.contains('@'));
        assert_eq!(key, RateLimiter::account_key("ana@teste.local"));
        assert_ne!(key, RateLimiter::account_key("outra@teste.local"));
    }

    #[test]
    fn chave_de_ip_e_estavel() {
        let key = RateLimiter::ip_key("203.0.113.10");
        assert!(key.starts_with(LOGIN_IP_PREFIX));
        assert!(!key.contains("203.0.113.10"));
        assert_eq!(key, RateLimiter::ip_key("203.0.113.10"));
    }

    #[test]
    fn chaves_de_reset_sao_separadas_das_de_login() {
        let account = RateLimiter::reset_account_key("ana@teste.local");
        let ip = RateLimiter::reset_ip_key("203.0.113.10");

        assert!(account.starts_with(RESET_ACCOUNT_PREFIX));
        assert!(ip.starts_with(RESET_IP_PREFIX));
        assert_ne!(account, RateLimiter::account_key("ana@teste.local"));
        assert!(RateLimiter::verification_key(uuid::Uuid::nil()).starts_with(VERIFY_PREFIX));
    }
}
