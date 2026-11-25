//! Confirmação de e-mail: código público e reenvio autenticado.

use axum::{extract::State, http::StatusCode};

use crate::{
    core::{
        auth::extractor::AuthUser, error::AppError, extract::ValidatedJson, openapi::Problem,
        state::AppState,
    },
    modules::auth::{dto::VerifyEmailRequest, service},
};

#[utoipa::path(
    post,
    path = "/auth/email/verify",
    tag = "auth",
    summary = "Confirma o e-mail com o código recebido",
    description = "O código é de uso único e vale 7 dias. Confirmar de novo uma \
                   conta já verificada devolve 204.",
    request_body = VerifyEmailRequest,
    responses(
        (status = 204, description = "E-mail confirmado"),
        (status = 400, description = "Código inválido ou expirado", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn verify(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<VerifyEmailRequest>,
) -> Result<StatusCode, AppError> {
    service::verify_email(&state, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/auth/email/resend",
    tag = "auth",
    summary = "Reenvia o código de confirmação",
    description = "Requer sessão válida. É limitado por usuário e não faz nada \
                   se o e-mail já está confirmado.",
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Pedido recebido"),
        (status = 401, description = "Não autenticado ou sessão expirada", body = Problem),
        (status = 429, description = "Muitos reenvios", body = Problem),
    )
)]
pub(super) async fn resend(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    service::resend_verification(&state, &auth).await?;
    Ok(StatusCode::NO_CONTENT)
}
