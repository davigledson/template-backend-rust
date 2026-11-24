//! DTOs de autenticação.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginRequest {
    #[validate(length(min = 1, max = 254))]
    pub email: String,
    #[validate(length(min = 1, max = 128))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefreshRequest {
    #[validate(length(min = 1, max = 512))]
    pub refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    /// Validade do access token em segundos.
    pub expires_in: i64,
}
