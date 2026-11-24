//! DTOs do módulo `users`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{
    core::{auth::role::Role, error::AppError},
    database::entity::users,
};

const DEFAULT_PAGE: u64 = 1;
const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

fn default_page() -> u64 {
    DEFAULT_PAGE
}

fn default_per_page() -> u64 {
    DEFAULT_PER_PAGE
}

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

/// Filtros da listagem. `active` ausente devolve ativos e inativos.
#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListUsersQuery {
    #[serde(default = "default_page")]
    #[validate(range(min = 1))]
    #[param(minimum = 1, default = 1)]
    pub page: u64,
    #[serde(default = "default_per_page")]
    #[validate(range(min = 1, max = 100))]
    #[param(minimum = 1, maximum = 100, default = 20)]
    pub per_page: u64,
    /// Busca por nome ou e-mail (case-insensitive).
    #[validate(length(max = 120))]
    pub search: Option<String>,
    pub role: Option<Role>,
    pub active: Option<bool>,
}

impl ListUsersQuery {
    pub fn page(&self) -> u64 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u64 {
        self.per_page.clamp(1, MAX_PER_PAGE)
    }

    pub fn offset(&self) -> u64 {
        self.page().saturating_sub(1) * self.per_page()
    }
}

/// Criação por administrador (não existe cadastro público).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateUserRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: String,
    #[validate(length(min = 3, max = 254))]
    pub email: String,
    #[validate(length(min = 12, max = 128))]
    pub password: String,
    /// Padrão: `user`.
    #[serde(default)]
    pub role: Role,
}

/// Atualização parcial. Campo ausente ou `null` não muda o valor atual.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateUserRequest {
    #[validate(length(min = 1, max = 120))]
    pub name: Option<String>,
    #[validate(length(min = 3, max = 254))]
    pub email: Option<String>,
    pub role: Option<Role>,
    pub active: Option<bool>,
}

/// Troca de senha feita por administrador.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 12, max = 128))]
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(page: u64, per_page: u64) -> ListUsersQuery {
        ListUsersQuery {
            page,
            per_page,
            search: None,
            role: None,
            active: None,
        }
    }

    #[test]
    fn paginacao_nunca_estoura_limites() {
        let q = query(0, 0);
        assert_eq!(q.page(), 1);
        assert_eq!(q.per_page(), 1);
        assert_eq!(q.offset(), 0);

        let q = query(2, 500);
        assert_eq!(q.page(), 2);
        assert_eq!(q.per_page(), 100);
        assert_eq!(q.offset(), 100);
    }

    #[test]
    fn rejeita_nome_vazio() {
        let request = CreateUserRequest {
            name: String::new(),
            email: "a@b.com".to_owned(),
            password: "uma-senha-bem-longa".to_owned(),
            role: Role::User,
        };
        assert!(request.validate().is_err());
    }
}
