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

/// Estado do outbox lido do banco pelo processo da API.
///
/// São gauges, não counters: `sent_24h` é uma janela rolável, então use
/// comparação entre coletas e não `rate()`.
pub fn set_outbox_depth(pending: u64, failed: u64, sent_24h: u64) {
    gauge!(OUTBOX_PENDING).set(pending as f64);
    gauge!(OUTBOX_FAILED).set(failed as f64);
    gauge!(OUTBOX_SENT_24H).set(sent_24h as f64);
}
