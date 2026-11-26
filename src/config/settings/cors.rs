//! CORS por ambiente. Produção começa sem origem permitida.

use axum::http::{HeaderName, HeaderValue, Method};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsSettings {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub max_age_secs: u64,
}

impl Default for CorsSettings {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allowed_methods: ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
                .map(str::to_owned)
                .to_vec(),
            allowed_headers: [
                "authorization",
                "content-type",
                "idempotency-key",
                "x-request-id",
            ]
            .map(str::to_owned)
            .to_vec(),
            max_age_secs: 3600,
        }
    }
}

impl CorsSettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.allowed_origins.iter().any(|origin| origin == "*") && self.allowed_origins.len() > 1
        {
            problems.push("cors.allowed_origins: use '*' sozinho ou liste origens".to_owned());
        }

        for origin in &self.allowed_origins {
            if origin != "*" && HeaderValue::from_str(origin).is_err() {
                problems.push(format!("cors.allowed_origins: origem inválida: {origin}"));
            }
        }

        for method in &self.allowed_methods {
            if method.parse::<Method>().is_err() {
                problems.push(format!("cors.allowed_methods: método inválido: {method}"));
            }
        }

        for header in &self.allowed_headers {
            if header.parse::<HeaderName>().is_err() {
                problems.push(format!(
                    "cors.allowed_headers: cabeçalho inválido: {header}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &CorsSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn aceita_configuracao_padrao() {
        assert!(errors(&CorsSettings::default()).is_empty());
    }

    #[test]
    fn rejeita_origem_metodo_e_cabecalho_invalidos() {
        let settings = CorsSettings {
            allowed_origins: vec!["*".to_owned(), "http://localhost:3000".to_owned()],
            allowed_methods: vec!["GET".to_owned(), "NAO EXISTE".to_owned()],
            allowed_headers: vec!["authorization".to_owned(), "nome inválido".to_owned()],
            ..Default::default()
        };

        let message = errors(&settings);
        assert!(message.contains("allowed_origins"), "{message}");
        assert!(message.contains("allowed_methods"), "{message}");
        assert!(message.contains("allowed_headers"), "{message}");
    }
}
