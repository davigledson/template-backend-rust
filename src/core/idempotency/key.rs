//! Quem pode repetir e qual resposta pertence a quem.
//!
//! A assinatura combina método, caminho, usuário (claim `sub` do access token)
//! e a chave crua. O hash SHA-256 é o que vai para o Redis; nada de token ou
//! chave em claro.

use axum::http::{HeaderMap, Method, header::AUTHORIZATION};
use sha2::{Digest, Sha256};

use crate::core::auth::jwt::JwtService;

const MAX_KEY_LEN: usize = 128;
const API_PREFIX: &str = "/api/v1/";
const SESSION_PREFIX: &str = "/api/v1/auth/";

/// `POST` de recurso exige a chave; rotas de sessão e demais métodos não.
pub(super) fn requires_key(method: &Method, path: &str) -> bool {
    method == Method::POST && path.starts_with(API_PREFIX) && !path.starts_with(SESSION_PREFIX)
}

/// Identifica o usuário pelo `sub` do access token. Um token inválido cai no
/// hash do cabeçalho: a rota vai responder `401`, mas as chaves não se misturam.
pub(super) fn subject(jwt: &JwtService, headers: &HeaderMap) -> String {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if let Some(token) = authorization.and_then(|value| value.strip_prefix("Bearer "))
        && let Ok(claims) = jwt.decode(token)
    {
        return format!("user:{}", claims.sub);
    }

    match authorization {
        Some(value) => format!("token:{}", hash(value)),
        None => "anonymous".to_owned(),
    }
}

pub(super) fn fingerprint(method: &str, path: &str, subject: &str, raw_key: &str) -> String {
    hash(&format!("{method}\n{path}\n{subject}\n{raw_key}"))
}

pub(super) fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_LEN && key.chars().all(|c| c.is_ascii_graphic())
}

fn hash(value: &str) -> String {
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
    fn chaves_invalidas_sao_recusadas() {
        assert!(valid_key("pedido-123"));
        assert!(!valid_key(""));
        assert!(!valid_key("com espaco"));
        assert!(!valid_key(&"a".repeat(MAX_KEY_LEN + 1)));
    }

    #[test]
    fn chave_e_exigida_so_em_post_de_recurso() {
        assert!(requires_key(&Method::POST, "/api/v1/users"));
        assert!(!requires_key(&Method::POST, "/api/v1/auth/login"));
        assert!(!requires_key(&Method::POST, "/api/v1/auth/refresh"));
        assert!(!requires_key(&Method::POST, "/metrics"));
        assert!(!requires_key(&Method::PUT, "/api/v1/users/1/password"));
        assert!(!requires_key(&Method::POST, "/health"));
    }

    #[test]
    fn fingerprint_separa_usuario_e_rota() {
        let base = fingerprint("POST", "/api/v1/users", "user:a", "k1");
        assert_eq!(base, fingerprint("POST", "/api/v1/users", "user:a", "k1"));
        assert_ne!(base, fingerprint("POST", "/api/v1/users", "user:b", "k1"));
        assert_ne!(base, fingerprint("POST", "/api/v1/users", "user:a", "k2"));
        assert_ne!(base, fingerprint("POST", "/api/v1/other", "user:a", "k1"));
        assert!(!base.contains("user:a"));
    }
}
