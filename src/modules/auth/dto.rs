//! DTOs de autenticação.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::auth::repository::SessionRecord;

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

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeOwnPasswordRequest {
    #[validate(length(min = 1, max = 128))]
    pub current_password: String,
    #[validate(length(min = 12, max = 128))]
    pub new_password: String,
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

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgotPasswordRequest {
    #[validate(length(min = 1, max = 254))]
    pub email: String,
}

/// Código recebido por e-mail. O código é de uso único e vale 30 minutos.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1, max = 200))]
    pub token: String,
    #[validate(length(min = 12, max = 128))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyEmailRequest {
    #[validate(length(min = 1, max = 200))]
    pub token: String,
}

/// Pedido público de reenvio do código de confirmação.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResendVerificationRequest {
    #[validate(length(min = 1, max = 254))]
    pub email: String,
}

/// Uma sessão ativa (família de refresh), como o usuário a vê.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionOut {
    pub family_id: Uuid,
    /// Sessão usada na requisição atual.
    pub current: bool,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ip: String,
    pub user_agent: Option<String>,
}

impl SessionOut {
    pub fn from_record(record: SessionRecord, current_family: Uuid) -> Self {
        Self {
            current: record.family_id == current_family,
            family_id: record.family_id,
            created_at: to_utc(record.created_at),
            last_seen_at: to_utc(record.last_seen_at),
            ip: record.ip,
            user_agent: record.user_agent,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub items: Vec<SessionOut>,
}

fn to_utc(epoch_ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(epoch_ms).unwrap_or_else(Utc::now)
}
