//! Endpoint `/metrics` (Prometheus).
//!
//! O token é opcional em desenvolvimento: vazio mantém o endpoint público, o
//! que só faz sentido quando a proxy/rede já restringe o acesso. Em produção,
//! com a rota ligada, `Settings::validate` exige o token.

use std::fmt;

use serde::Deserialize;

pub const MIN_TOKEN_LEN: usize = 16;

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct MetricsSettings {
    /// Monta (ou não) a rota `/metrics`.
    pub enabled: bool,
    /// Token Bearer; obrigatório em produção quando a rota está ligada.
    /// Nunca aparece no `Debug`.
    pub token: String,
}

impl MetricsSettings {
    /// Token efetivo, ignorando espaços nas pontas.
    pub fn token(&self) -> Option<&str> {
        let token = self.token.trim();
        (!token.is_empty()).then_some(token)
    }

    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.enabled
            && let Some(token) = self.token()
            && token.len() < MIN_TOKEN_LEN
        {
            problems.push(format!(
                "metrics.token deve ter ao menos {MIN_TOKEN_LEN} caracteres quando definido"
            ));
        }
    }
}

impl Default for MetricsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            token: String::new(),
        }
    }
}

impl fmt::Debug for MetricsSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricsSettings")
            .field("enabled", &self.enabled)
            .field(
                "token",
                &if self.token.trim().is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &MetricsSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn aceita_token_vazio_ou_forte() {
        assert!(errors(&MetricsSettings::default()).is_empty());

        let settings = MetricsSettings {
            token: "token-de-teste-123456".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).is_empty());
    }

    #[test]
    fn rejeita_token_curto_quando_habilitado() {
        let settings = MetricsSettings {
            token: "curto".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("metrics.token"));

        let disabled = MetricsSettings {
            enabled: false,
            token: "curto".to_owned(),
        };
        assert!(errors(&disabled).is_empty());
    }

    #[test]
    fn token_nao_aparece_no_debug() {
        let settings = MetricsSettings {
            token: "segredo-de-metricas".to_owned(),
            ..Default::default()
        };

        let debug = format!("{settings:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("segredo-de-metricas"));
    }
}
