//! DTOs do módulo `users`.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    core::{auth::role::Role, error::AppError},
    database::entity::users,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub active: bool,
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
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        })
    }
}
