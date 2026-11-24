//! Servidor HTTP: endereço, limites e confiança no proxy.

use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    /// `0` deixa o Tokio escolher. Em servidor de 2 vCPU use `2`.
    pub worker_threads: usize,
    pub body_limit_mb: usize,
    /// Confia em `X-Forwarded-For`/`X-Real-IP` (apenas atrás de proxy conhecido).
    pub trust_proxy_headers: bool,
}

impl ServerSettings {
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    pub fn body_limit_bytes(&self) -> usize {
        self.body_limit_mb.saturating_mul(1024 * 1024)
    }

    /// `None` quando `worker_threads = 0`.
    pub fn worker_threads(&self) -> Option<usize> {
        (self.worker_threads > 0).then_some(self.worker_threads)
    }

    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.port == 0 {
            problems.push("server.port deve ser maior que zero".to_owned());
        }
        if self.request_timeout_secs == 0 {
            problems.push("server.request_timeout_secs deve ser maior que zero".to_owned());
        }
        if self.body_limit_mb == 0 {
            problems.push("server.body_limit_mb deve ser maior que zero".to_owned());
        }
    }
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            request_timeout_secs: 30,
            worker_threads: 0,
            body_limit_mb: 2,
            trust_proxy_headers: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &ServerSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_limites_zero() {
        assert!(errors(&ServerSettings::default()).is_empty());

        let settings = ServerSettings {
            port: 0,
            request_timeout_secs: 0,
            body_limit_mb: 0,
            ..Default::default()
        };
        let message = errors(&settings);
        assert!(message.contains("server.port"), "{message}");
        assert!(message.contains("request_timeout_secs"), "{message}");
        assert!(message.contains("body_limit_mb"), "{message}");
    }

    #[test]
    fn converte_limites_para_runtime() {
        let settings = ServerSettings::default();
        assert_eq!(settings.request_timeout(), Duration::from_secs(30));
        assert_eq!(settings.body_limit_bytes(), 2 * 1024 * 1024);
        assert_eq!(settings.worker_threads(), None);
    }
}
