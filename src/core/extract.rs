//! Extratores que validam o corpo/query antes do handler.
//!
//! O erro de validação vira [`AppError::Validation`] (422 em Problem Details),
//! diferente do `Json`/`Query` padrão do Axum, que responderia texto puro.

use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::request::Parts,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::core::error::AppError;

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
