//! Resposta gravada no Redis: lock, replay e allowlist de cabeçalhos.

use axum::{
    body::Body,
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode,
        header::{CONTENT_TYPE, ETAG, LOCATION},
    },
    response::Response,
};
use serde::{Deserialize, Serialize};

use crate::core::{cache::Cache, error::AppError};

const PREFIX: &str = "idem:";
const PROCESSING: &str = "processing";
const TTL_SECS: i64 = 24 * 60 * 60;
const LOCK_SECS: i64 = 60;

/// Cabeçalhos de resposta preservados no replay. `date`, `content-length` e
/// `set-cookie` ficam de fora: não fazem sentido fora da resposta original.
const REPLAYED_HEADERS: [HeaderName; 3] = [CONTENT_TYPE, LOCATION, ETAG];

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StoredResponse {
    pub(super) status: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: String,
}

pub(super) enum Acquired {
    Lock,
    Replay(StoredResponse),
    Busy,
}

pub(super) async fn acquire(cache: &Cache, fingerprint: &str) -> Result<Acquired, AppError> {
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

pub(super) async fn save(
    cache: &Cache,
    fingerprint: &str,
    stored: &StoredResponse,
) -> Result<(), AppError> {
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

pub(super) async fn delete(cache: &Cache, fingerprint: &str) {
    let Ok(mut connection) = cache.pool().get().await else {
        return;
    };

    let _: Result<i64, _> = redis::cmd("DEL")
        .arg(key_for(fingerprint))
        .query_async(&mut connection)
        .await;
}

/// Reconstrói a resposta original a partir do que foi gravado.
pub(super) fn replay(stored: StoredResponse) -> Response {
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
        .unwrap_or_else(|_| super::internal_response())
}

pub(super) fn replayable_headers(headers: &HeaderMap) -> Vec<(String, String)> {
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

fn key_for(fingerprint: &str) -> String {
    format!("{PREFIX}{fingerprint}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
