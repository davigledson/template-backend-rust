//! Envio de e-mail atrás de uma abstração.
//!
//! - `log`: desenvolvimento, não envia nada e não registra o endereço completo;
//! - `smtp`: envio real via `lettre`.
//!
//! O envio nunca acontece no caminho da requisição: o módulo `email` grava o
//! e-mail em um outbox transacional e o worker entrega.

mod log;
mod smtp;

use std::sync::Arc;

use async_trait::async_trait;
use lettre::{address::AddressError, message::Mailbox, transport::smtp::Error as SmtpError};
use thiserror::Error;

use crate::config::settings::MailSettings;

pub use log::LogMailer;
pub use smtp::SmtpMailer;

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Error)]
pub enum MailerError {
    #[error("configuração de e-mail inválida: {0}")]
    Config(String),
    #[error("endereço de e-mail inválido: {0}")]
    Address(#[from] AddressError),
    #[error("mensagem inválida: {0}")]
    Message(#[from] lettre::error::Error),
    #[error("falha no envio: {0}")]
    Send(#[from] SmtpError),
    #[error("falha no envio: {0}")]
    Transport(String),
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailerError>;
}

/// Constrói o mailer a partir da configuração.
pub fn from_settings(settings: &MailSettings) -> Result<Arc<dyn Mailer>, MailerError> {
    match settings.provider.as_str() {
        "log" => Ok(Arc::new(LogMailer::new(settings.from.clone()))),
        "smtp" => {
            let from: Mailbox = settings.from.parse()?;
            Ok(Arc::new(SmtpMailer::new(settings.smtp_url.trim(), from)?))
        }
        other => Err(MailerError::Config(format!(
            "provider desconhecido: {other} (use log ou smtp)"
        ))),
    }
}

/// Mascara o endereço para log sem vazar o dado completo.
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() => {
            let first = local.chars().next().unwrap_or('*');
            format!("{first}***@{domain}")
        }
        _ => "***".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: &str) -> MailSettings {
        MailSettings {
            provider: provider.to_owned(),
            from: "a@b.com".to_owned(),
            smtp_url: String::new(),
        }
    }

    #[test]
    fn mascara_email() {
        assert_eq!(mask_email("ana@teste.local"), "a***@teste.local");
        assert_eq!(mask_email("a@b.com"), "a***@b.com");
        assert_eq!(mask_email("sem-arroba"), "***");
    }

    #[test]
    fn provider_log_nao_exige_smtp() {
        assert!(from_settings(&settings("log")).is_ok());
    }

    #[test]
    fn provider_desconhecido_falha() {
        assert!(from_settings(&settings("carrier-pigeon")).is_err());
    }
}
