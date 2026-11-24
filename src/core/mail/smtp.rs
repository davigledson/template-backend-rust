//! Provider `smtp`: envio real via `lettre`.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};

use crate::core::mail::{EmailMessage, Mailer, MailerError};

pub struct SmtpMailer {
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpMailer {
    pub(super) fn new(smtp_url: &str, from: Mailbox) -> Result<Self, MailerError> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
            .map_err(|error| MailerError::Config(error.to_string()))?
            .build();

        Ok(Self { from, transport })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
