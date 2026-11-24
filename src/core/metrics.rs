//! Métricas Prometheus.
//!
//! O recorder é global e só pode ser instalado uma vez por processo; o
//! `OnceLock` permite montar o router mais de uma vez (testes) sem panic.
//! O endpoint `/metrics` é montado em `app::build` fora da camada de medição,
//! para não contar a si mesmo.

use std::sync::OnceLock;

use axum_prometheus::{
    PrometheusMetricLayer, PrometheusMetricLayerBuilder, metrics,
    metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle},
};

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
