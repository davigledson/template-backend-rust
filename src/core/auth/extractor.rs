//! Extractor `AuthUser`: valida o Bearer token e expõe as permissões.
//!
//! O extractor é stateless (não consulta o banco). Rotas sensíveis chamam
//! `users::service::require_session` para verificar `token_version`, papel e
//! `active` no banco.

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::core::{
    auth::{permission::Permission, role::Role},
    error::AppError,
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub role: Role,
    pub token_version: i32,
}

impl AuthUser {
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if self.role.can(permission) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "permissão insuficiente: {}",
                permission.as_str()
            )))
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("token de acesso ausente".to_owned()))?;

        let token = header
            .strip_prefix("Bearer ")
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| AppError::Unauthorized("formato do token inválido".to_owned()))?;

        let claims = state
            .jwt
            .decode(token)
            .map_err(|_| AppError::Unauthorized("token inválido ou expirado".to_owned()))?;

        let id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AppError::Unauthorized("token inválido".to_owned()))?;

        Ok(Self {
            id,
            email: claims.email,
            role: claims.role,
            token_version: claims.token_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::auth::permission::USERS_DELETE;

    fn user(role: Role) -> AuthUser {
        AuthUser {
            id: Uuid::now_v7(),
            email: "a@b.com".to_owned(),
            role,
            token_version: 0,
        }
    }

    #[test]
    fn admin_passa_em_permissao_de_admin() {
        assert!(user(Role::Admin).require(USERS_DELETE).is_ok());
    }

    #[test]
    fn usuario_comum_recebe_forbidden() {
        let error = user(Role::User).require(USERS_DELETE).unwrap_err();
        assert_eq!(error.status(), axum::http::StatusCode::FORBIDDEN);
    }
}
