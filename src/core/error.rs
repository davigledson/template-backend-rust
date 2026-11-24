//! Erro único da aplicação, convertido em Problem Details (RFC 9457).
//!
//! Regras:
//! - `4xx` devolve a mensagem para o cliente;
//! - `5xx` é registrado no log e devolve uma mensagem genérica;
//! - validação devolve `422` com a lista de campos inválidos.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    MethodNotAllowed(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("erro de validação")]
    Validation(#[from] validator::ValidationErrors),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Cache(#[from] crate::core::cache::CacheError),
    #[error(transparent)]
    Password(#[from] crate::core::auth::password::PasswordError),
    #[error(transparent)]
    Token(#[from] crate::core::auth::jwt::JwtError),
    #[error("erro interno: {0}")]
    Internal(String),
}

impl From<deadpool_redis::PoolError> for AppError {
    fn from(error: deadpool_redis::PoolError) -> Self {
        Self::Cache(crate::core::cache::CacheError::Pool(error))
    }
}

impl From<redis::RedisError> for AppError {
    fn from(error: redis::RedisError) -> Self {
        Self::Cache(crate::core::cache::CacheError::Redis(error))
    }
}

impl AppError {
    /// Erro interno: o conteúdo vai para o log, nunca para a resposta.
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self::Internal(error.to_string())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::MethodNotAllowed(_) => StatusCode::METHOD_NOT_ALLOWED,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Database(_)
            | Self::Cache(_)
            | Self::Password(_)
            | Self::Token(_)
            | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        let (title, detail) = match &self {
            Self::BadRequest(detail) => ("Requisição inválida", detail.clone()),
            Self::Unauthorized(detail) => ("Não autenticado", detail.clone()),
            Self::Forbidden(detail) => ("Acesso negado", detail.clone()),
            Self::NotFound(detail) => ("Recurso não encontrado", detail.clone()),
            Self::Conflict(detail) => ("Conflito", detail.clone()),
            Self::MethodNotAllowed(detail) => ("Método não permitido", detail.clone()),
            Self::Unavailable(detail) => ("Serviço indisponível", detail.clone()),
            Self::Validation(errors) => {
                let mut problem = problemdetails::new(StatusCode::UNPROCESSABLE_ENTITY)
                    .with_title("Dados inválidos")
                    .with_detail("A requisição contém campos inválidos.");
                if let Ok(value) = serde_json::to_value(errors) {
                    problem = problem.with_value("errors", value);
                }
                return problem.into_response();
            }
            Self::Database(_)
            | Self::Cache(_)
            | Self::Password(_)
            | Self::Token(_)
            | Self::Internal(_) => {
                tracing::error!(error = %self, "erro interno");
                (
                    "Erro interno",
                    "Não foi possível concluir a operação.".to_owned(),
                )
            }
        };

        problemdetails::new(status)
            .with_title(title)
            .with_detail(detail)
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    #[tokio::test]
    async fn not_found_vira_problem_json() {
        let response = AppError::NotFound("produto inexistente".to_owned()).into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("content-type").map(|v| v.as_bytes()),
            Some(b"application/problem+json".as_slice())
        );

        let body = to_bytes(response.into_body(), 4096)
            .await
            .unwrap_or_default();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        assert_eq!(json["title"], "Recurso não encontrado");
        assert_eq!(json["detail"], "produto inexistente");
    }

    #[tokio::test]
    async fn erro_de_banco_nao_vaza_detalhe() {
        let error = AppError::Database(sea_orm::DbErr::Query(sea_orm::RuntimeErr::Internal(
            "senha do banco na mensagem".to_owned(),
        )));

        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = to_bytes(response.into_body(), 4096)
            .await
            .unwrap_or_default();
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("senha do banco"), "{text}");
        assert!(text.contains("Não foi possível concluir"), "{text}");
    }
}
