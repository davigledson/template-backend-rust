//! CORS por ambiente. Produção começa sem origem permitida.

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
