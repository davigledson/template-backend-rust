//! Emissão e validação do access token.

use std::{fmt, time::Duration};

use serde::Deserialize;

pub(super) const MIN_SECRET_LEN: usize = 32;

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct JwtSettings {
    pub issuer: String,
    pub audience: String,
    pub access_ttl_min: u64,
    pub refresh_ttl_days: u64,
    pub secret: String,
}

impl JwtSettings {
    pub fn access_ttl(&self) -> Duration {
        Duration::from_secs(self.access_ttl_min.saturating_mul(60))
    }

    pub fn refresh_ttl(&self) -> Duration {
        Duration::from_secs(self.refresh_ttl_days.saturating_mul(24 * 60 * 60))
    }

    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.secret.len() < MIN_SECRET_LEN {
            problems.push(format!(
                "jwt.secret deve ter ao menos {MIN_SECRET_LEN} caracteres (defina APP__JWT__SECRET)"
            ));
        }
        if self.access_ttl_min == 0 {
            problems.push("jwt.access_ttl_min deve ser maior que zero".to_owned());
        }
        if self.refresh_ttl_days == 0 {
            problems.push("jwt.refresh_ttl_days deve ser maior que zero".to_owned());
        }
    }
}

impl Default for JwtSettings {
    fn default() -> Self {
        Self {
            issuer: "template-api".to_owned(),
            audience: "template-front".to_owned(),
            access_ttl_min: 15,
            refresh_ttl_days: 7,
            secret: String::new(),
        }
    }
}

// O segredo nunca deve aparecer em log.
impl fmt::Debug for JwtSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtSettings")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("access_ttl_min", &self.access_ttl_min)
            .field("refresh_ttl_days", &self.refresh_ttl_days)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &JwtSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    fn valid() -> JwtSettings {
        JwtSettings {
            secret: "x".repeat(MIN_SECRET_LEN),
            ..Default::default()
        }
    }

    #[test]
    fn rejeita_segredo_curto() {
        assert!(errors(&valid()).is_empty());

        let settings = JwtSettings {
            secret: "curto".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("jwt.secret"));
    }

    #[test]
    fn rejeita_ttl_zero() {
        let settings = JwtSettings {
            access_ttl_min: 0,
            refresh_ttl_days: 0,
            ..valid()
        };

        let message = errors(&settings);
        assert!(message.contains("access_ttl_min"), "{message}");
        assert!(message.contains("refresh_ttl_days"), "{message}");
    }

    #[test]
    fn segredo_nao_aparece_no_debug() {
        let settings = JwtSettings {
            secret: "segredo-que-nao-pode-vazar-1234567890".to_owned(),
            ..valid()
        };

        let debug = format!("{settings:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("segredo-que-nao-pode-vazar"));
    }
}
