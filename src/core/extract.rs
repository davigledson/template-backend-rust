//! Extratores que validam o corpo/query antes do handler.
//!
//! O erro de validação vira [`AppError::Validation`] (422 em Problem Details),
//! diferente do `Json`/`Query` padrão do Axum, que responderia texto puro.

use std::convert::Infallible;

use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::{HeaderName, request::Parts},
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::core::{error::AppError, http::REQUEST_ID_HEADER};

/// Cabeçalho `x-request-id` da requisição atual.
///
/// Usado para correlacionar auditoria com o log, sem depender de span.
pub struct RequestId(pub Option<String>);

impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let name = HeaderName::from_static(REQUEST_ID_HEADER);
        let value = parts
            .headers
            .get(&name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        Ok(Self(value))
    }
}

/// `Json<T>` com validação do `validator`.
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|error| AppError::BadRequest(error.body_text()))?;

        value.validate()?;
        Ok(Self(value))
    }
}

/// `Query<T>` com validação do `validator`.
pub struct ValidatedQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|error| AppError::BadRequest(error.body_text()))?;

        value.validate()?;
        Ok(Self(value))
    }
}
