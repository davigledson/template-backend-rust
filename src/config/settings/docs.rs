//! Documentação OpenAPI (`/docs` e `/api-docs/openapi.json`).
//!
//! Em produção pública, desligue para não expor o contrato nem a UI.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DocsSettings {
    /// Expõe `/docs`, `/api-docs/openapi.json` e o favicon.
    pub enabled: bool,
}

impl Default for DocsSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}
