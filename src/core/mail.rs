//! Envio de e-mail atrás de uma abstração.
//!
//! - `log`: desenvolvimento, não envia nada e não registra o endereço completo;
//! - `smtp`: envio real via `lettre`.
//!
//! O envio nunca acontece no caminho da requisição: o módulo `email` grava o
//! e-mail em um outbox transacional e o worker entrega.

use std::sync::Arc;

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    address::AddressError,
    message::{Mailbox, header::ContentType},
    transport::smtp::Error as SmtpError,
};
use thiserror::Error;

use crate::config::settings::MailSettings;

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
        "log" => Ok(Arc::new(LogMailer {
            from: settings.from.clone(),
        })),
        "smtp" => {
            let from: Mailbox = settings.from.parse()?;
            let transport =
                AsyncSmtpTransport::<Tokio1Executor>::from_url(settings.smtp_url.trim())
                    .map_err(|error| MailerError::Config(error.to_string()))?
                    .build();

            Ok(Arc::new(SmtpMailer { from, transport }))
        }
        other => Err(MailerError::Config(format!(
            "provider desconhecido: {other} (use log ou smtp)"
        ))),
    }
}

pub struct LogMailer {
    from: String,
}

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailerError> {
        tracing::info!(
            from = %self.from,
            to = %mask_email(&message.to),
            subject = %message.subject,
            "e-mail simulado (mail.provider=log)"
        );
        Ok(())
    }
}

pub struct SmtpMailer {
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailerError> {
        let email = build_message(&self.from, message)?;

        self.transport
            .send(email)
            .await
            .map_err(|error| MailerError::Transport(error.to_string()))?;

        Ok(())
    }
}

fn build_message(from: &Mailbox, message: &EmailMessage) -> Result<Message, MailerError> {
    Message::builder()
        .from(from.clone())
        .to(message.to.parse::<Mailbox>()?)
        .subject(message.subject.clone())
        // Sem o charset explícito o cliente pode interpretar o corpo como
        // Latin-1 e exibir "OlÃ¡" no lugar de "Olá".
        .header(ContentType::TEXT_PLAIN)
        .body(message.body.clone())
        .map_err(MailerError::from)
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

    #[test]
    fn mascara_email() {
        assert_eq!(mask_email("ana@teste.local"), "a***@teste.local");
        assert_eq!(mask_email("a@b.com"), "a***@b.com");
        assert_eq!(mask_email("sem-arroba"), "***");
    }

    #[test]
    fn provider_log_nao_exige_smtp() {
        let settings = MailSettings {
            provider: "log".to_owned(),
            from: "a@b.com".to_owned(),
            smtp_url: String::new(),
        };
        assert!(from_settings(&settings).is_ok());
    }

    #[test]
    fn mensagem_declara_utf8() {
        let from: Mailbox = "nao-responda@teste.local".parse().expect("remetente");
        let message = EmailMessage {
            to: "ana@teste.local".to_owned(),
            subject: "Sua conta foi criada".to_owned(),
            body: "Olá, Ana.".to_owned(),
        };

        let built = build_message(&from, &message).expect("mensagem");
        let raw = String::from_utf8(built.formatted()).expect("utf-8");

        assert!(
            raw.contains("Content-Type: text/plain; charset=utf-8"),
            "{raw}"
        );
        assert!(raw.contains("Ol=C3=A1"), "{raw}");
    }

    #[test]
    fn provider_desconhecido_falha() {
        let settings = MailSettings {
            provider: "carrier-pigeon".to_owned(),
            from: "a@b.com".to_owned(),
            smtp_url: String::new(),
        };
        assert!(from_settings(&settings).is_err());
    }
}
