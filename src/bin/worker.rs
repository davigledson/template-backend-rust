//! Binário do worker (cron).
//!
//! Fase 0: apenas inicializa o tracing. O scheduler entra na Fase 6.

use std::process::ExitCode;

use tracing_subscriber::{EnvFilter, fmt};

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(true).init();
}

fn main() -> ExitCode {
    init_tracing();
    tracing::info!(
        app = template_backend_rust::APP_NAME,
        version = template_backend_rust::APP_VERSION,
        "worker iniciado em modo esqueleto (fase 0)"
    );
    ExitCode::SUCCESS
}
