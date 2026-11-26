//! Tokens opacos de confirmação de e-mail no Redis.
//!
//! Ficam em `core` porque dois módulos usam: `users` emite (criação e troca de
//! e-mail) e `auth` consome (rota pública de confirmação).
//!
//! - guardado apenas como hash SHA-256, com TTL de 7 dias;
//! - uso único: o consumo é um `GET` + `DEL` atômico;
//! - o valor carrega `token_version` (trocar senha/papel invalida) e o
//!   fingerprint do e-mail (trocar o endereço invalida o código pendente);
//! - só o último código emitido vale: um reenvio apaga o anterior.

use uuid::Uuid;

use crate::core::{
    auth::hash::{email_fingerprint, sha256_hex},
    cache::Cache,
    error::AppError,
};

const PREFIX: &str = "auth:email-verify:";
const TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Emite o novo código, apaga o anterior do usuário e grava o ponteiro do atual.
const ISSUE_SCRIPT: &str = r"
local old = redis.call('GET', KEYS[1])
if old then
  redis.call('DEL', ARGV[1] .. old)
end
redis.call('SET', KEYS[1], ARGV[2], 'EX', ARGV[3])
redis.call('SET', KEYS[2], ARGV[4], 'EX', ARGV[3])
return 1
";

/// `DEL` do token e do ponteiro; devolve 0 se outra chamada já consumiu.
const CONSUME_SCRIPT: &str = r"
if redis.call('DEL', KEYS[1]) == 0 then return 0 end
redis.call('DEL', KEYS[2])
return 1
";

/// Gera e guarda um token novo; devolve o valor em claro para o e-mail.
pub async fn issue(
    cache: &Cache,
    user_id: Uuid,
    token_version: i32,
    email: &str,
) -> Result<String, AppError> {
    let token = generate_token();
    let token_hash = sha256_hex(&token);
    let value = format!("{user_id}:{token_version}:{}", email_fingerprint(email));
    let mut connection = cache.pool().get().await?;

    let _: i64 = redis::Script::new(ISSUE_SCRIPT)
        .key(latest_key_for(user_id))
        .key(token_key(&token_hash))
        .arg(PREFIX)
        .arg(&token_hash)
        .arg(TTL_SECS)
        .arg(value)
        .invoke_async(&mut connection)
        .await?;

    Ok(token)
}

/// Valida e invalida o token. `None` se expirado, já usado, desconhecido ou
/// substituído por um reenvio mais novo.
pub async fn consume(cache: &Cache, token: &str) -> Result<Option<(Uuid, i32, String)>, AppError> {
    let token_hash = sha256_hex(token);
    let token_key = token_key(&token_hash);
    let mut connection = cache.pool().get().await?;

    let value: Option<String> = redis::cmd("GET")
        .arg(&token_key)
        .query_async(&mut connection)
        .await?;

    let Some(parsed) = value.as_deref().and_then(parse_value) else {
        return Ok(None);
    };

    let latest_key = latest_key_for(parsed.0);
    let latest: Option<String> = redis::cmd("GET")
        .arg(&latest_key)
        .query_async(&mut connection)
        .await?;

    if latest.as_deref() != Some(token_hash.as_str()) {
        return Ok(None);
    }

    let removed: i64 = redis::Script::new(CONSUME_SCRIPT)
        .key(&token_key)
        .key(&latest_key)
        .invoke_async(&mut connection)
        .await?;

    if removed == 0 {
        return Ok(None);
    }

    Ok(Some(parsed))
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

fn token_key(token_hash: &str) -> String {
    format!("{PREFIX}{token_hash}")
}

fn latest_key_for(user_id: Uuid) -> String {
    format!("{PREFIX}latest:{user_id}")
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
        let key = token_key(&sha256_hex(&token));
        assert!(!key.contains(&token));
        assert!(key.starts_with(PREFIX));
    }

    #[test]
    fn ponteiro_do_ultimo_codigo_e_por_usuario() {
        let user_id = Uuid::now_v7();
        assert_eq!(latest_key_for(user_id), format!("{PREFIX}latest:{user_id}"));
        assert!(!latest_key_for(user_id).contains(&sha256_hex("token")));
    }
}
