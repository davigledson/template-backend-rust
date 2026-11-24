//! Idempotência de `POST` com `Idempotency-Key`.
//!
//! A chave é opcional: sem o cabeçalho, a requisição segue normalmente. Com
//! ele, a primeira execução grava a resposta no Redis por 24 h e as repetições
//! devolvem exatamente a mesma resposta, sem repetir o efeito.
//!
//! - a assinatura inclui método, caminho e hash do `Authorization`, para uma
//!   chave não vazar a resposta de outro usuário;
//! - enquanto a primeira execução está em andamento, a chave vale "processing"
//!   e uma repetição recebe `409`;
//! - resposta `5xx` não é gravada: o cliente pode tentar de novo;
//! - Redis fora do ar faz a requisição seguir sem idempotência (fail-open).

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{Method, StatusCode, header::CONTENT_TYPE},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::{cache::Cache, error::AppError, state::AppState};

const PREFIX: &str = "idem:";
const PROCESSING: &str = "processing";
const TTL_SECS: i64 = 24 * 60 * 60;
const LOCK_SECS: i64 = 60;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_LEN: usize = 128;

#[derive(Debug, Serialize, Deserialize)]
struct StoredResponse {
    status: u16,
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    body: String,
}

pub async fn middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }

    let Some(raw_key) = request
        .headers()
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    else {
        return next.run(request).await;
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
        request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        raw_key,
    );

    let cache = &state.cache;
    match acquire(cache, &fingerprint).await {
        Ok(Acquired::Replay(stored)) => {
            return build_response(
                StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK),
                stored.content_type.as_deref(),
                axum::body::Bytes::from(stored.body),
            )
            .unwrap_or_else(|_| internal_response());
        }
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
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let body = match to_bytes(response.into_body(), MAX_RESPONSE_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(error = %error, "resposta grande demais para idempotência");
            delete(cache, &fingerprint).await;
            return internal_response();
        }
    };

    if status.is_server_error() {
        delete(cache, &fingerprint).await;
    } else if let Ok(text) = String::from_utf8(body.to_vec()) {
        let stored = StoredResponse {
            status: status.as_u16(),
            content_type: content_type.clone(),
            body: text,
        };

        if let Err(error) = save(cache, &fingerprint, &stored).await {
            tracing::warn!(error = %error, "falha ao gravar a resposta idempotente");
        }
    }

    build_response(status, content_type.as_deref(), body).unwrap_or_else(|_| internal_response())
}

enum Acquired {
    Lock,
    Replay(StoredResponse),
    Busy,
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

fn fingerprint(method: &str, path: &str, authorization: Option<&str>, raw_key: &str) -> String {
    let auth = authorization.unwrap_or("anonymous");
    hash(&format!("{method}\n{path}\n{auth}\n{raw_key}"))
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

fn build_response(
    status: StatusCode,
    content_type: Option<&str>,
    body: axum::body::Bytes,
) -> Result<Response, axum::http::Error> {
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }
    builder.body(Body::from(body))
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
    fn fingerprint_separa_usuario_e_rota() {
        let base = fingerprint("POST", "/api/v1/users", Some("Bearer a"), "k1");
        assert_eq!(
            base,
            fingerprint("POST", "/api/v1/users", Some("Bearer a"), "k1")
        );
        assert_ne!(
            base,
            fingerprint("POST", "/api/v1/users", Some("Bearer b"), "k1")
        );
        assert_ne!(
            base,
            fingerprint("POST", "/api/v1/users", Some("Bearer a"), "k2")
        );
        assert_ne!(
            base,
            fingerprint("POST", "/api/v1/other", Some("Bearer a"), "k1")
        );
        assert!(!base.contains("Bearer a"));
    }
}
