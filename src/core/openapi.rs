//! Tipos usados apenas na documentação OpenAPI.

use serde::Serialize;
use utoipa::ToSchema;

/// Corpo de erro RFC 9457 (`application/problem+json`).
///
/// Não é construído manualmente pela API — o `problemdetails` monta a resposta.
/// Este tipo existe para o schema e para o Scalar mostrar os campos.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// URI que identifica o tipo do problema.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Erros por campo (apenas em 422).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}
