//! Confirmação de e-mail: código público e reenvio público por e-mail.

use axum::{extract::State, http::StatusCode};

use crate::{
    core::{
        client_ip::ClientIp, error::AppError, extract::ValidatedJson, openapi::Problem,
        state::AppState,
    },
    modules::auth::{
        dto::{ResendVerificationRequest, VerifyEmailRequest},
        service,
    },
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
    description = "Sempre devolve 204, exista ou não a conta ou o e-mail já \
                   esteja confirmado, para não permitir enumeração. O pedido é \
                   limitado por conta e por IP.",
    request_body = ResendVerificationRequest,
    responses(
        (status = 204, description = "Pedido recebido"),
        (status = 400, description = "JSON inválido", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
        (status = 429, description = "Muitos reenvios", body = Problem),
    )
)]
pub(super) async fn resend(
    State(state): State<AppState>,
    client_ip: ClientIp,
    ValidatedJson(body): ValidatedJson<ResendVerificationRequest>,
) -> Result<StatusCode, AppError> {
    service::resend_verification(&state, body, client_ip.as_str()).await?;
    Ok(StatusCode::NO_CONTENT)
}
