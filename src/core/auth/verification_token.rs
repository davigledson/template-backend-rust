//! Tokens opacos de confirmação de e-mail no Redis.
//!
//! Ficam em `core` porque dois módulos usam: `users` emite (criação e troca de
//! e-mail) e `auth` consome (rota pública de confirmação).
//!
//! - guardado apenas como hash SHA-256, com TTL de 7 dias;
//! - uso único: o consumo é um `GET` + `DEL` atômico.

use uuid::Uuid;

use crate::core::{auth::hash::sha256_hex, cache::Cache, error::AppError};

const PREFIX: &str = "auth:email-verify:";
const TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Consome o token e devolve o usuário dono, se existir.
const CONSUME_SCRIPT: &str = r"
local value = redis.call('GET', KEYS[1])
if not value then return nil end
redis.call('DEL', KEYS[1])
return value
";

/// Gera e guarda um token novo; devolve o valor em claro para o e-mail.
pub async fn issue(cache: &Cache, user_id: Uuid) -> Result<String, AppError> {
    let token = generate_token();
    let mut connection = cache.pool().get().await?;

    let _: String = redis::cmd("SET")
        .arg(key_for(&token))
        .arg(user_id.to_string())
        .arg("EX")
        .arg(TTL_SECS)
        .query_async(&mut connection)
        .await?;

    Ok(token)
}

/// Valida e invalida o token. `None` se expirado, já usado ou desconhecido.
pub async fn consume(cache: &Cache, token: &str) -> Result<Option<Uuid>, AppError> {
    let mut connection = cache.pool().get().await?;

    let value: Option<String> = redis::Script::new(CONSUME_SCRIPT)
        .key(key_for(token))
        .invoke_async(&mut connection)
        .await?;

    Ok(value.and_then(|value| Uuid::parse_str(&value).ok()))
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn key_for(token: &str) -> String {
    format!("{PREFIX}{}", sha256_hex(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chave_nao_guarda_token_em_claro() {
        let token = generate_token();
        assert!(!key_for(&token).contains(&token));
        assert!(key_for(&token).starts_with(PREFIX));
    }
}
