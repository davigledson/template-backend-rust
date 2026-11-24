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
//!
//! O código se divide em `key` (quem pode repetir e qual assinatura) e `store`
//! (lock, gravação e replay no Redis).

mod key;
mod store;

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::core::{error::AppError, state::AppState};
use key::{fingerprint, requires_key, subject, valid_key};
use store::{Acquired, StoredResponse, acquire, delete, replayable_headers, save};

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

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
        Ok(Acquired::Replay(stored)) => return store::replay(stored),
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

pub(super) fn internal_response() -> Response {
    AppError::internal("falha ao processar idempotência").into_response()
}
