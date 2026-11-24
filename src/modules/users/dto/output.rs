//! Representação de saída: nunca inclui hash de senha.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    core::{auth::role::Role, error::AppError},
    database::entity::users,
};

/// Usuário como a API expõe (nunca inclui hash de senha).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub active: bool,
    /// Versão para concorrência otimista.
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UserOut {
    pub fn from_model(model: users::Model) -> Result<Self, AppError> {
        let role = Role::parse(&model.role).ok_or_else(|| {
            AppError::internal(format!("papel inválido no banco: {}", model.role))
        })?;

        Ok(Self {
            id: model.id,
            name: model.name,
            email: model.email,
            role,
            active: model.active,
            version: model.version,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        })
    }
}

/// Lista paginada de usuários.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserPage {
    pub items: Vec<UserOut>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
}

impl UserPage {
    pub fn new(items: Vec<UserOut>, page: u64, per_page: u64, total: u64) -> Self {
        Self {
            items,
            page,
            per_page,
            total,
        }
    }
}
