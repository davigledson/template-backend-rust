//! Binário do worker (cron).
//!
//! Fase 1: carrega configuração e logging. O scheduler entra na Fase 6.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use template_backend_rust::{config::Settings, core::telemetry};

fn main() -> ExitCode {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = telemetry::init(&settings.log) {
        eprintln!("falha ao iniciar o logging: {error}");
        return ExitCode::FAILURE;
    }

    tracing::info!(
        profile = %settings.profile,
        timezone = %settings.jobs.timezone,
        jobs_enabled = settings.jobs.enabled,
        "worker pronto (sem jobs registrados — fase 6)"
    );

    ExitCode::SUCCESS
}
