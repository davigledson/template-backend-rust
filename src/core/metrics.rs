//! Métricas Prometheus.
//!
//! O recorder é global e só pode ser instalado uma vez por processo; o
//! `OnceLock` permite montar o router mais de uma vez (testes) sem panic.
//! O endpoint `/metrics` é montado em `app::build` fora da camada de medição,
//! para não contar a si mesmo.
//!
//! As métricas de domínio ficam centralizadas aqui, atrás de funções: assim o
//! nome e os labels são um contrato único, e nenhum label carrega PII ou valor
//! de alta cardinalidade.

use std::sync::OnceLock;

use axum::{
    http::{
        HeaderMap, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE},
    },
    response::{IntoResponse, Response},
};
use axum_prometheus::{
    PrometheusMetricLayer, PrometheusMetricLayerBuilder, metrics,
    metrics::{counter, gauge},
    metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle},
};

pub const LOGIN_BLOCKED: &str = "app_login_blocked_total";
pub const LOGIN_FAILURE: &str = "app_login_failure_total";
pub const REFRESH_REUSE: &str = "app_refresh_reuse_total";
pub const IDEMPOTENCY_REPLAY: &str = "app_idempotency_replay_total";
pub const IDEMPOTENCY_CONFLICT: &str = "app_idempotency_conflict_total";
pub const OUTBOX_PENDING: &str = "app_email_outbox_pending";
pub const OUTBOX_FAILED: &str = "app_email_outbox_failed";
pub const OUTBOX_SENT_24H: &str = "app_email_sent_24h";
pub const PASSWORD_RESET_REQUESTED: &str = "app_password_reset_requested_total";
pub const PASSWORD_RESET_BLOCKED: &str = "app_password_reset_blocked_total";
pub const PASSWORD_RESET_COMPLETED: &str = "app_password_reset_completed_total";
pub const EMAIL_VERIFIED: &str = "app_email_verified_total";
pub const SESSION_REVOKED: &str = "app_session_revoked_total";

pub fn pair() -> (PrometheusMetricLayer<'static>, PrometheusHandle) {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

    let handle = HANDLE
        .get_or_init(|| {
            let recorder = PrometheusBuilder::new().build_recorder();
            let handle = recorder.handle();

            if let Err(error) = metrics::set_global_recorder(recorder) {
                tracing::error!(error = %error, "falha ao instalar o recorder de métricas");
            }

            handle
        })
        .clone();

    let layer = PrometheusMetricLayerBuilder::new().build();
    (layer, handle)
}

/// Responde `/metrics`, exigindo Bearer quando um token foi configurado.
pub fn render(handle: PrometheusHandle, expected: Option<&str>, headers: &HeaderMap) -> Response {
    if let Some(expected) = expected
        && !authorized(headers, expected)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, "Bearer")],
            "token de métricas inválido",
        )
            .into_response();
    }

    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4")],
        handle.render(),
    )
        .into_response()
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(token) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };

    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

/// Compara sem sair no primeiro byte diferente.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }

    difference == 0
}

/// Login recusado antes de conferir a senha. `scope` é `account` ou `ip`.
pub fn login_blocked(scope: &'static str) {
    counter!(LOGIN_BLOCKED, "scope" => scope).increment(1);
}

pub fn login_failure() {
    counter!(LOGIN_FAILURE).increment(1);
}

/// Refresh já consumido reapresentado; a família foi revogada.
pub fn refresh_reuse() {
    counter!(REFRESH_REUSE).increment(1);
}

pub fn idempotency_replay() {
    counter!(IDEMPOTENCY_REPLAY).increment(1);
}

/// Mesma chave ainda em execução (resposta `409`).
pub fn idempotency_conflict() {
    counter!(IDEMPOTENCY_CONFLICT).increment(1);
}

/// Pedido de recuperação recusado pelo rate limit. `scope` é `account` ou `ip`.
pub fn password_reset_blocked(scope: &'static str) {
    counter!(PASSWORD_RESET_BLOCKED, "scope" => scope).increment(1);
}

/// Código de recuperação emitido para uma conta existente.
pub fn password_reset_requested() {
    counter!(PASSWORD_RESET_REQUESTED).increment(1);
}

pub fn password_reset_completed() {
    counter!(PASSWORD_RESET_COMPLETED).increment(1);
}

/// E-mail passou de não confirmado para confirmado (transição, não repetição).
pub fn email_verified() {
    counter!(EMAIL_VERIFIED).increment(1);
}

/// Sessão encerrada pelo usuário (logout ou tela de dispositivos).
pub fn session_revoked() {
    counter!(SESSION_REVOKED).increment(1);
}

/// Estado do outbox lido do banco pelo processo da API.
///
/// São gauges, não counters: `sent_24h` é uma janela rolável, então use
/// comparação entre coletas e não `rate()`.
pub fn set_outbox_depth(pending: u64, failed: u64, sent_24h: u64) {
    gauge!(OUTBOX_PENDING).set(pending as f64);
    gauge!(OUTBOX_FAILED).set(failed as f64);
    gauge!(OUTBOX_SENT_24H).set(sent_24h as f64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn compara_token_em_tempo_constante() {
        assert!(constant_time_eq(b"segredo", b"segredo"));
        assert!(!constant_time_eq(b"segredo", b"segreda"));
        assert!(!constant_time_eq(b"segredo", b"segred"));
    }

    #[test]
    fn aceita_apenas_bearer_correto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer token-de-teste"),
        );
        assert!(authorized(&headers, "token-de-teste"));

        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer outro"));
        assert!(!authorized(&headers, "token-de-teste"));
        assert!(!authorized(&HeaderMap::new(), "token-de-teste"));
    }
}
