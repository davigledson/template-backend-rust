//! Tokens opacos de confirmação de e-mail no Redis.
//!
//! Ficam em `core` porque dois módulos usam: `users` emite (criação e troca de
//! e-mail) e `auth` consome (rota pública de confirmação).
//!
//! - guardado apenas como hash SHA-256, com TTL de 7 dias;
//! - uso único: o consumo é um `GET` + `DEL` atômico;
//! - o valor carrega `token_version` (trocar senha/papel invalida) e o
//!   fingerprint do e-mail (trocar o endereço invalida o código pendente).

use uuid::Uuid;

use crate::core::{
    auth::hash::{email_fingerprint, sha256_hex},
    cache::Cache,
    error::AppError,
};

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
pub async fn issue(
    cache: &Cache,
    user_id: Uuid,
    token_version: i32,
    email: &str,
) -> Result<String, AppError> {
    let token = generate_token();
    let mut connection = cache.pool().get().await?;

    let _: String = redis::cmd("SET")
        .arg(key_for(&token))
        .arg(format!(
            "{user_id}:{token_version}:{}",
            email_fingerprint(email)
        ))
        .arg("EX")
        .arg(TTL_SECS)
        .query_async(&mut connection)
        .await?;

    Ok(token)
}

/// Valida e invalida o token. `None` se expirado, já usado ou desconhecido.
pub async fn consume(cache: &Cache, token: &str) -> Result<Option<(Uuid, i32, String)>, AppError> {
    let mut connection = cache.pool().get().await?;

    let value: Option<String> = redis::Script::new(CONSUME_SCRIPT)
        .key(key_for(token))
        .invoke_async(&mut connection)
        .await?;

    Ok(value.as_deref().and_then(parse_value))
}

fn parse_value(value: &str) -> Option<(Uuid, i32, String)> {
    let mut parts = value.split(':');
    let user_id = Uuid::parse_str(parts.next()?).ok()?;
    let token_version = parts.next()?.parse().ok()?;
    let fingerprint = parts.next()?.to_owned();

    if parts.next().is_some() {
        return None;
    }

    Some((user_id, token_version, fingerprint))
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
    fn valor_ida_e_volta() {
        let user_id = Uuid::now_v7();
        let fingerprint = email_fingerprint("ana@teste.local");
        let value = format!("{user_id}:4:{fingerprint}");

        assert_eq!(parse_value(&value), Some((user_id, 4, fingerprint)));
        assert_eq!(parse_value("sem-dois-pontos"), None);
        assert_eq!(parse_value("nao-uuid:4:abcd"), None);
        assert_eq!(parse_value("nao-uuid"), None);
        assert_eq!(parse_value(&format!("{user_id}:x:abcd")), None);
        assert_eq!(parse_value(&format!("{user_id}:1:abcd:extra")), None);
    }

    #[test]
    fn chave_nao_guarda_token_em_claro() {
        let token = generate_token();
        assert!(!key_for(&token).contains(&token));
        assert!(key_for(&token).starts_with(PREFIX));
    }
}
