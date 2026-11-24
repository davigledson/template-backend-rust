//! Envio de e-mail: `log` (desenvolvimento) ou `smtp`.

use std::fmt;

use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct MailSettings {
    /// `log` (desenvolvimento) ou `smtp`.
    pub provider: String,
    pub from: String,
    /// URL do SMTP; nunca aparece no `Debug`.
    pub smtp_url: String,
}

impl MailSettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if !matches!(self.provider.as_str(), "log" | "smtp") {
            problems.push("mail.provider deve ser log ou smtp".to_owned());
        }
        if self.from.trim().is_empty() || !self.from.contains('@') {
            problems.push("mail.from precisa ser um e-mail válido".to_owned());
        }
        if self.provider == "smtp" && self.smtp_url.trim().is_empty() {
            problems.push("mail.smtp_url é obrigatório quando mail.provider = smtp".to_owned());
        }
    }
}

impl Default for MailSettings {
    fn default() -> Self {
        Self {
            provider: "log".to_owned(),
            from: "nao-responda@example.com".to_owned(),
            smtp_url: String::new(),
        }
    }
}

impl fmt::Debug for MailSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailSettings")
            .field("provider", &self.provider)
            .field("from", &self.from)
            .field("smtp_url", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &MailSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_provider_desconhecido() {
        assert!(errors(&MailSettings::default()).is_empty());

        let settings = MailSettings {
            provider: "pombo".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("mail.provider"));
    }

    #[test]
    fn smtp_exige_url() {
        let settings = MailSettings {
            provider: "smtp".to_owned(),
            smtp_url: String::new(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("mail.smtp_url"));
    }

    #[test]
    fn rejeita_remetente_invalido() {
        let settings = MailSettings {
            from: "nao-responda".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("mail.from"));
    }

    #[test]
    fn url_do_smtp_nao_aparece_no_debug() {
        let settings = MailSettings {
            provider: "smtp".to_owned(),
            smtp_url: "smtps://usuario:senha-secreta@smtp.example.com:465".to_owned(),
            ..Default::default()
        };

        let debug = format!("{settings:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("senha-secreta"));
    }
}
