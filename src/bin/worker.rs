//! Binário do worker (cron): entrega de e-mails do outbox e limpeza.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::ExitCode;

use template_backend_rust::{config::Settings, core::telemetry, jobs};

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

    let runtime = match build_runtime(&settings) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("falha ao criar o runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(jobs::run(settings)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, "worker encerrado com erro");
            ExitCode::FAILURE
        }
    }
}

fn build_runtime(settings: &Settings) -> Result<tokio::runtime::Runtime, std::io::Error> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if let Some(workers) = settings.server.worker_threads() {
        builder.worker_threads(workers);
    }
    builder.build()
}
