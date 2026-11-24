//! Corpos de entrada das rotas de `users`.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

use crate::core::auth::role::Role;

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
