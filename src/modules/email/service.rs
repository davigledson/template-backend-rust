//! Regras do outbox de e-mail.
//!
//! - `enqueue` grava a intenção na **mesma transação** da mudança de negócio;
//! - `process_pending` entrega com retry e backoff, fora da requisição.
//!
//! A entrega é at-least-once: o envio precisa ser idempotente do lado do
//! provedor. O `dedupe_key` evita duplicar a linha do outbox.

use chrono::{Duration, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, Set};
use uuid::Uuid;

use crate::{
    core::{
        error::AppError,
        mail::{EmailMessage, Mailer},
        metrics,
        state::AppState,
    },
    database::entity::email_outbox,
    modules::email::repository,
};

pub const MAX_ATTEMPTS: i32 = 5;
pub const RETENTION_DAYS: i64 = 30;
pub const BATCH_SIZE: u64 = 20;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct OutboxDepth {
    pub pending: u64,
    pub failed: u64,
    pub sent_24h: u64,
}

/// Intervalo de atualização dos gauges de outbox no processo da API.
const DEPTH_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProcessSummary {
    pub claimed: usize,
    pub sent: usize,
    pub retried: usize,
    pub failed: usize,
}

/// Grava um e-mail pendente. `dedupe_key` é única por evento.
pub async fn enqueue<C>(conn: &C, message: EmailMessage, dedupe_key: String) -> Result<(), AppError>
where
    C: ConnectionTrait,
{
    let now = Utc::now().fixed_offset();
    let model = email_outbox::ActiveModel {
        id: Set(Uuid::now_v7()),
        to_email: Set(message.to),
        subject: Set(message.subject),
        body: Set(message.body),
        status: Set("pending".to_owned()),
        attempts: Set(0),
        next_attempt_at: Set(now),
        locked_at: Set(None),
        last_error: Set(None),
        dedupe_key: Set(dedupe_key),
        created_at: Set(now),
        sent_at: Set(None),
    };

    repository::enqueue(conn, model).await?;
    Ok(())
}

/// Processa um lote de e-mails pendentes.
#[tracing::instrument(skip(db, mailer), fields(batch = limit))]
pub async fn process_pending(
    db: &DatabaseConnection,
    mailer: &dyn Mailer,
    limit: u64,
) -> Result<ProcessSummary, AppError> {
    let items = repository::claim_batch(db, limit).await?;
    let mut summary = ProcessSummary {
        claimed: items.len(),
        ..ProcessSummary::default()
    };

    for item in items {
        let message = EmailMessage {
            to: item.to_email.clone(),
            subject: item.subject.clone(),
            body: item.body.clone(),
        };

        match mailer.send(&message).await {
            Ok(()) => {
                repository::mark_sent(db, item.id).await?;
                summary.sent += 1;
            }
            Err(error) => {
                let error_text = truncate(&error.to_string(), 500);

                if item.attempts >= MAX_ATTEMPTS {
                    repository::mark_failed(db, item.id, &error_text).await?;
                    tracing::warn!(
                        outbox_id = %item.id,
                        attempts = item.attempts,
                        "e-mail falhou após o limite de tentativas"
                    );
                    summary.failed += 1;
                } else {
                    let next = Utc::now().fixed_offset()
                        + Duration::minutes(backoff_minutes(item.attempts));
                    repository::mark_retry(db, item.id, next, &error_text).await?;
                    tracing::warn!(
                        outbox_id = %item.id,
                        attempts = item.attempts,
                        "falha ao enviar e-mail; reagendado"
                    );
                    summary.retried += 1;
                }
            }
        }
    }

    Ok(summary)
}

/// Profundidade atual do outbox, usada pelas métricas e pelo runbook.
pub async fn depth(db: &DatabaseConnection) -> Result<OutboxDepth, AppError> {
    let day_ago = Utc::now().fixed_offset() - Duration::hours(24);

    Ok(OutboxDepth {
        pending: repository::count_by_status(db, "pending").await?,
        failed: repository::count_by_status(db, "failed").await?,
        sent_24h: repository::count_sent_since(db, day_ago).await?,
    })
}

/// Publica os gauges de outbox a cada 30 s no processo da API.
///
/// O worker não expõe `/metrics`, então medir lá seria inútil: o scrape
/// acontece no processo HTTP, que lê o estado do banco.
pub fn spawn_depth_reporter(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(DEPTH_INTERVAL_SECS));

        loop {
            ticker.tick().await;

            match depth(&state.db).await {
                Ok(depth) => {
                    metrics::set_outbox_depth(depth.pending, depth.failed, depth.sent_24h);
                }
                Err(error) => tracing::warn!(error = %error, "falha ao medir o outbox"),
            }
        }
    });
}

/// Lista itens de um status para inspeção (`pending` ou `failed`).
pub async fn list(
    db: &DatabaseConnection,
    status: &str,
    limit: u64,
) -> Result<Vec<email_outbox::Model>, AppError> {
    Ok(repository::list_by_status(db, status, limit).await?)
}

/// Devolve um item ao processamento imediato. `false` se não havia o que fazer.
pub async fn retry(db: &DatabaseConnection, id: Uuid) -> Result<bool, AppError> {
    Ok(repository::requeue(db, id).await? > 0)
}

/// Reprocessa todos os itens de um status; `force` zera o contador de tentativas.
pub async fn retry_all(
    db: &DatabaseConnection,
    status: &str,
    force: bool,
) -> Result<u64, AppError> {
    Ok(repository::requeue_all(db, status, force).await?)
}

/// Remove e-mails enviados há mais de `RETENTION_DAYS`.
pub async fn purge_sent(db: &DatabaseConnection) -> Result<u64, AppError> {
    let cutoff = Utc::now().fixed_offset() - Duration::days(RETENTION_DAYS);
    Ok(repository::delete_sent_before(db, cutoff).await?)
}

/// Backoff exponencial em minutos, com teto de 60.
pub fn backoff_minutes(attempts: i32) -> i64 {
    let exponent = (attempts.max(1) - 1).min(6) as u32;
    2_i64.pow(exponent).min(60)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated: String = value.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_cresce_e_tem_teto() {
        assert_eq!(backoff_minutes(1), 1);
        assert_eq!(backoff_minutes(2), 2);
        assert_eq!(backoff_minutes(3), 4);
        assert_eq!(backoff_minutes(4), 8);
        assert_eq!(backoff_minutes(5), 16);
        assert_eq!(backoff_minutes(10), 60);
    }

    #[test]
    fn trunca_erro_longo_sem_quebrar_utf8() {
        let value = "á".repeat(10);
        let result = truncate(&value, 5);
        assert_eq!(result.chars().count(), 6);
        assert!(result.ends_with('…'));
    }
}
