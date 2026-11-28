//! Jobs de background. Rodam somente no binário `worker`.

use std::{sync::Arc, time::Instant};

use sea_orm::DatabaseConnection;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{
    config::Settings,
    core::{error::AppError, mail, shutdown},
    database::connection,
    modules::{audit::service as audit_service, email::service as email_service},
};

const OUTBOX_SCHEDULE: &str = "*/15 * * * * *";
const CLEANUP_SCHEDULE: &str = "0 0 3 * * *";

pub async fn run(settings: Settings) -> Result<(), AppError> {
    if !settings.jobs.enabled {
        // Sair com sucesso faria o `restart: unless-stopped` do Compose
        // reiniciar o container em loop. Fica vivo até o encerramento.
        tracing::info!("jobs desabilitados (jobs.enabled = false); aguardando encerramento");
        shutdown::signal().await;
        return Ok(());
    }

    let timezone = settings
        .jobs
        .timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|error| AppError::internal(format!("timezone inválido: {error}")))?;

    let db = connection::connect(&settings.database).await?;
    let mailer = mail::from_settings(&settings.mail).map_err(AppError::internal)?;
    let mut scheduler = JobScheduler::new().await.map_err(AppError::internal)?;

    let outbox_db = db.clone();
    let outbox_mailer = Arc::clone(&mailer);
    scheduler
        .add(
            Job::new_async_tz(OUTBOX_SCHEDULE, timezone, move |_id, _lock| {
                let db = outbox_db.clone();
                let mailer = Arc::clone(&outbox_mailer);
                Box::pin(async move {
                    process_outbox(&db, mailer.as_ref()).await;
                })
            })
            .map_err(AppError::internal)?,
        )
        .await
        .map_err(AppError::internal)?;

    let cleanup_db = db.clone();
    let audit_retention_days = settings.audit.retention_days;
    scheduler
        .add(
            Job::new_async_tz(CLEANUP_SCHEDULE, timezone, move |_id, _lock| {
                let db = cleanup_db.clone();
                Box::pin(async move {
                    cleanup(&db, audit_retention_days).await;
                })
            })
            .map_err(AppError::internal)?,
        )
        .await
        .map_err(AppError::internal)?;

    scheduler.start().await.map_err(AppError::internal)?;
    tracing::info!(
        outbox = OUTBOX_SCHEDULE,
        cleanup = CLEANUP_SCHEDULE,
        timezone = %settings.jobs.timezone,
        "worker iniciado"
    );

    shutdown::signal().await;
    scheduler.shutdown().await.map_err(AppError::internal)?;
    tracing::info!("worker encerrado com sucesso");
    Ok(())
}

#[tracing::instrument(skip(db, mailer))]
async fn process_outbox(db: &DatabaseConnection, mailer: &dyn mail::Mailer) {
    let started = Instant::now();

    match email_service::process_pending(db, mailer, email_service::BATCH_SIZE).await {
        Ok(summary) => {
            if summary.claimed > 0 {
                tracing::info!(
                    claimed = summary.claimed,
                    sent = summary.sent,
                    retried = summary.retried,
                    failed = summary.failed,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "outbox processado"
                );
            }
        }
        Err(error) => {
            tracing::error!(error = %error, "falha ao processar o outbox");
        }
    }
}

async fn cleanup(db: &DatabaseConnection, audit_retention_days: u64) {
    purge_sent(db).await;
    purge_failed(db).await;
    purge_audit(db, audit_retention_days).await;
}

async fn purge_sent(db: &DatabaseConnection) {
    match email_service::purge_sent(db).await {
        Ok(removed) if removed > 0 => {
            tracing::info!(removed, "outbox antigo removido");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(error = %error, "falha ao limpar o outbox");
        }
    }
}

async fn purge_failed(db: &DatabaseConnection) {
    match email_service::purge_failed(db).await {
        Ok(removed) if removed > 0 => {
            tracing::info!(removed, "outbox failed antigo removido");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(error = %error, "falha ao limpar outbox failed");
        }
    }
}

async fn purge_audit(db: &DatabaseConnection, retention_days: u64) {
    match audit_service::purge(db, retention_days).await {
        Ok(removed) if removed > 0 => {
            tracing::info!(removed, retention_days, "auditoria antiga removida");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(error = %error, "falha ao limpar a auditoria");
        }
    }
}
