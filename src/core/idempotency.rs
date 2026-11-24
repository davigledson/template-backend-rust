//! Idempotência de `POST` de recurso com `Idempotency-Key`.
//!
//! - a chave é **obrigatória** em `POST` sob `/api/v1` (exceto `/api/v1/auth/`);
//! - rotas de sessão ficam de fora: a resposta carrega tokens e um replay
//!   devolveria um refresh já rotacionado, o que dispararia a detecção de reuso;
//! - a assinatura inclui método, caminho, usuário (claim `sub` do access token)
//!   e a chave, para uma chave não vazar a resposta de outro usuário;
//! - a primeira execução grava a resposta por 24 h e as repetições devolvem
//!   status, `content-type`, `location`, `etag` e corpo originais;
//! - enquanto a primeira execução está em andamento, a chave vale "processing"
//!   e uma repetição recebe `409`;
//! - resposta `5xx` não é gravada: o cliente pode tentar de novo;
//! - Redis fora do ar faz a requisição seguir sem idempotência (fail-open).

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, ETAG, LOCATION},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::{auth::jwt::JwtService, cache::Cache, error::AppError, state::AppState};

const PREFIX: &str = "idem:";
const PROCESSING: &str = "processing";
const TTL_SECS: i64 = 24 * 60 * 60;
const LOCK_SECS: i64 = 60;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_LEN: usize = 128;
const API_PREFIX: &str = "/api/v1/";
const SESSION_PREFIX: &str = "/api/v1/auth/";

/// Cabeçalhos de resposta preservados no replay. `date`, `content-length` e
/// `set-cookie` ficam de fora: não fazem sentido fora da resposta original.
const REPLAYED_HEADERS: [HeaderName; 3] = [CONTENT_TYPE, LOCATION, ETAG];

#[derive(Debug, Serialize, Deserialize)]
struct StoredResponse {
    status: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<(String, String)>,
    body: String,
}

pub async fn middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !requires_key(request.method(), request.uri().path()) {
        return next.run(request).await;
    }

    let Some(raw_key) = request
        .headers()
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    else {
        return AppError::BadRequest("Idempotency-Key é obrigatório em POST de recurso".to_owned())
            .into_response();
    };

    if !valid_key(raw_key) {
        return AppError::BadRequest(
            "Idempotency-Key deve ter de 1 a 128 caracteres ASCII visíveis".to_owned(),
        )
        .into_response();
    }

    let fingerprint = fingerprint(
        request.method().as_str(),
        request.uri().path(),
        &subject(&state.jwt, request.headers()),
        raw_key,
    );

    let cache = &state.cache;
    match acquire(cache, &fingerprint).await {
        Ok(Acquired::Replay(stored)) => return replay(stored),
        Ok(Acquired::Busy) => {
            return AppError::Conflict(
                "já existe uma requisição em andamento com esta Idempotency-Key".to_owned(),
            )
            .into_response();
        }
        Ok(Acquired::Lock) => {}
        Err(error) => {
            tracing::warn!(error = %error, "Redis indisponível; seguindo sem idempotência");
            return next.run(request).await;
        }
    }

    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let status = parts.status;

    let body = match to_bytes(body, MAX_RESPONSE_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(error = %error, "resposta grande demais para idempotência");
            delete(cache, &fingerprint).await;
            return internal_response();
        }
    };

    if status.is_server_error() {
        delete(cache, &fingerprint).await;
    } else {
        match String::from_utf8(body.to_vec()) {
            Ok(text) => {
                let stored = StoredResponse {
                    status: status.as_u16(),
                    headers: replayable_headers(&parts.headers),
                    body: text,
                };

                if let Err(error) = save(cache, &fingerprint, &stored).await {
                    tracing::warn!(error = %error, "falha ao gravar a resposta idempotente");
                }
            }
            Err(_) => {
                tracing::warn!("resposta não textual; não será reaproveitada");
                delete(cache, &fingerprint).await;
            }
        }
    }

    Response::from_parts(parts, Body::from(body))
}

enum Acquired {
    Lock,
    Replay(StoredResponse),
    Busy,
}

/// `POST` de recurso exige a chave; rotas de sessão e demais métodos não.
fn requires_key(method: &Method, path: &str) -> bool {
    method == Method::POST && path.starts_with(API_PREFIX) && !path.starts_with(SESSION_PREFIX)
}

/// Identifica o usuário pelo `sub` do access token. Um token inválido cai no
/// hash do cabeçalho: a rota vai responder `401`, mas as chaves não se misturam.
fn subject(jwt: &JwtService, headers: &HeaderMap) -> String {
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

async fn acquire(cache: &Cache, fingerprint: &str) -> Result<Acquired, AppError> {
    let mut connection = cache.pool().get().await?;
    let key = key_for(fingerprint);

    let value: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg(PROCESSING)
        .arg("NX")
        .arg("EX")
        .arg(LOCK_SECS)
        .query_async(&mut connection)
        .await?;

    if value.is_some() {
        return Ok(Acquired::Lock);
    }

    let existing: Option<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut connection)
        .await?;

    match existing.as_deref() {
        Some(PROCESSING) | None => Ok(Acquired::Busy),
        Some(value) => match serde_json::from_str::<StoredResponse>(value) {
            Ok(stored) => Ok(Acquired::Replay(stored)),
            Err(error) => {
                tracing::warn!(error = %error, "resposta idempotente corrompida");
                Ok(Acquired::Busy)
            }
        },
    }
}

async fn save(cache: &Cache, fingerprint: &str, stored: &StoredResponse) -> Result<(), AppError> {
    let mut connection = cache.pool().get().await?;
    let value = serde_json::to_string(stored).map_err(AppError::internal)?;

    let _: String = redis::cmd("SET")
        .arg(key_for(fingerprint))
        .arg(value)
        .arg("EX")
        .arg(TTL_SECS)
        .query_async(&mut connection)
        .await?;

    Ok(())
}

async fn delete(cache: &Cache, fingerprint: &str) {
    let Ok(mut connection) = cache.pool().get().await else {
        return;
    };

    let _: Result<i64, _> = redis::cmd("DEL")
        .arg(key_for(fingerprint))
        .query_async(&mut connection)
        .await;
}

fn key_for(fingerprint: &str) -> String {
    format!("{PREFIX}{fingerprint}")
}

fn fingerprint(method: &str, path: &str, subject: &str, raw_key: &str) -> String {
    hash(&format!("{method}\n{path}\n{subject}\n{raw_key}"))
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

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_LEN && key.chars().all(|c| c.is_ascii_graphic())
}

fn replayable_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    REPLAYED_HEADERS
        .iter()
        .filter_map(|name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}

fn replay(stored: StoredResponse) -> Response {
    let status = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);

    for (name, value) in &stored.headers {
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) else {
            continue;
        };
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(stored.body))
        .unwrap_or_else(|_| internal_response())
}

fn internal_response() -> Response {
    AppError::internal("falha ao processar idempotência").into_response()
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

    #[test]
    fn replay_preserva_apenas_headers_da_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(LOCATION, HeaderValue::from_static("/api/v1/users/1"));
        headers.insert("x-interno", HeaderValue::from_static("nao-vaza"));

        let mut saved = replayable_headers(&headers);
        saved.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(
            saved,
            vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("location".to_owned(), "/api/v1/users/1".to_owned()),
            ]
        );
    }
}
