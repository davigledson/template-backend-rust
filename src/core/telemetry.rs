//! Inicialização do tracing.
//!
//! `RUST_LOG` tem prioridade; sem ele, usa `log.level` da configuração.
//! `log.format = "json"` é o padrão de produção.

use thiserror::Error;
use tracing_subscriber::{
    EnvFilter,
    filter::ParseError,
    fmt,
    layer::SubscriberExt,
    util::{SubscriberInitExt, TryInitError},
};

use crate::config::settings::LogSettings;

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("filtro de log inválido: {0}")]
    Filter(#[from] ParseError),
    #[error("não foi possível registrar o subscriber: {0}")]
    Init(#[from] TryInitError),
}

pub fn init(settings: &LogSettings) -> Result<(), TelemetryError> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::try_new(&settings.level)?,
    };

    let registry = tracing_subscriber::registry().with(filter);

    match settings.format.as_str() {
        "json" => registry
            .with(
                fmt::layer()
                    .json()
                    .with_ansi(settings.ansi)
                    .with_target(true),
            )
            .try_init()?,
        "compact" => registry
            .with(
                fmt::layer()
                    .compact()
                    .with_ansi(settings.ansi)
                    .with_target(true),
            )
            .try_init()?,
        _ => registry
            .with(
                fmt::layer()
                    .pretty()
                    .with_ansi(settings.ansi)
                    .with_target(true),
            )
            .try_init()?,
    }

    Ok(())
}
