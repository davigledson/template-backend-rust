//! Handlers de autenticação.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    core::{auth::extractor::AuthUser, error::AppError, state::AppState},
    modules::{
        auth::{
            dto::{LoginRequest, RefreshRequest, TokenResponse},
            service,
        },
        users::{dto::UserOut, service as users_service},
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(service::login(&state, body).await?))
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    Ok(Json(service::refresh(&state, body).await?))
}

async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<StatusCode, AppError> {
    service::logout(&state, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<Json<UserOut>, AppError> {
    let user = users_service::require_session(&state.db, &auth).await?;
    Ok(Json(UserOut::from_model(user)?))
}
