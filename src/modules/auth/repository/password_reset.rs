//! Tokens de recuperação de senha: 30 minutos, uso único, hash no Redis.

use uuid::Uuid;

use crate::{
    core::{
        auth::hash::{email_fingerprint, sha256_hex},
        cache::Cache,
        error::AppError,
    },
    modules::auth::repository::token::generate_token,
};

const PREFIX: &str = "auth:pwreset:";
const TTL_SECS: i64 = 30 * 60;

/// `GET` + `DEL` no mesmo passo: o token nunca é usado duas vezes.
const CONSUME_SCRIPT: &str = r"
local value = redis.call('GET', KEYS[1])
if not value then return nil end
redis.call('DEL', KEYS[1])
return value
";

/// Guarda o token e devolve o valor em claro para o e-mail.
///
/// O valor carrega `token_version` (trocar senha/papel invalida) e o
/// fingerprint do e-mail (trocar o e-mail invalida o código pendente).
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

/// Consome o token. `None` se expirado, usado ou desconhecido.
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
    }

    #[test]
    fn chave_nao_guarda_token_em_claro() {
        let token = generate_token();
        assert!(!key_for(&token).contains(&token));
    }
}
