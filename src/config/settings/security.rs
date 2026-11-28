//! Política de segurança aplicada no login e nos cabeçalhos HTTP.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Exige e-mail confirmado para entrar. Precisa de e-mail transacional
    /// configurado, porque a confirmação chega por e-mail.
    pub require_email_verified: bool,
    /// `Strict-Transport-Security` em segundos. `0` desliga o cabeçalho.
    /// Só faz efeito quando a API é servida por HTTPS.
    pub hsts_max_age_secs: u64,
    /// Envia o cookie de refresh com `Secure`. Em produção deve ser `true`;
    /// desenvolvimento local em HTTP usa `false`.
    pub cookie_secure: bool,
    /// `SameSite` do cookie de refresh: `strict`, `lax` ou `none`.
    ///
    /// `none` só funciona com `cookie_secure = true` e é obrigatório quando o
    /// front está em outro site (ex.: Vercel + API em domínio próprio).
    pub cookie_same_site: String,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            require_email_verified: false,
            hsts_max_age_secs: 0,
            cookie_secure: true,
            cookie_same_site: "strict".to_owned(),
        }
    }
}

impl SecuritySettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if !matches!(self.cookie_same_site.as_str(), "strict" | "lax" | "none") {
            problems.push("security.cookie_same_site deve ser strict, lax ou none".to_owned());
        }

        if self.cookie_same_site == "none" && !self.cookie_secure {
            problems.push(
                "security.cookie_same_site = none exige security.cookie_secure = true".to_owned(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &SecuritySettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn aceita_padrao() {
        assert!(errors(&SecuritySettings::default()).is_empty());
    }

    #[test]
    fn rejeita_same_site_desconhecido() {
        let settings = SecuritySettings {
            cookie_same_site: "sempre".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("cookie_same_site"));
    }

    #[test]
    fn same_site_none_exige_secure() {
        let settings = SecuritySettings {
            cookie_same_site: "none".to_owned(),
            cookie_secure: false,
            ..Default::default()
        };
        assert!(errors(&settings).contains("cookie_secure"));
    }
}
