//! Binário da API HTTP.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{process::ExitCode, sync::Arc};

use template_backend_rust::{
    app,
    config::Settings,
    core::{
        auth::jwt::JwtService, cache::Cache, error::AppError, mail, shutdown, state::AppState,
        telemetry,
    },
    database::connection,
};
use tokio::net::TcpListener;

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

    match runtime.block_on(run(settings)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, "api encerrada com erro");
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

async fn run(settings: Settings) -> Result<(), AppError> {
    let config = Arc::new(settings);

    tracing::info!(
        profile = %config.profile,
        version = template_backend_rust::APP_VERSION,
        "iniciando api"
    );

    let db = connection::connect(&config.database).await?;
    let cache = Cache::connect(&config.redis)?;
    let jwt = JwtService::new(&config.jwt)?;
    let mailer = mail::from_settings(&config.mail).map_err(AppError::internal)?;
    let state = AppState::new(db, cache, jwt, mailer, Arc::clone(&config));
    let router = app::build(state);

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&address)
        .await
        .map_err(AppError::internal)?;

    tracing::info!(address = %address, "api ouvindo");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown::signal())
        .await
        .map_err(AppError::internal)?;

    tracing::info!("api encerrada com sucesso");
    Ok(())
}
