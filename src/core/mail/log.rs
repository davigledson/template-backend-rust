//! Provider `log`: desenvolvimento, sem SMTP e sem endereço completo no log.

use async_trait::async_trait;

use crate::core::mail::{EmailMessage, Mailer, MailerError, mask_email};

pub struct LogMailer {
    from: String,
}

impl LogMailer {
    pub(super) fn new(from: String) -> Self {
        Self { from }
    }
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
