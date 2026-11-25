//! Recuperação de senha: pedido por e-mail e confirmação com o código.

use axum::{extract::State, http::StatusCode};

use crate::{
    core::{
        client_ip::ClientIp, error::AppError, extract::ValidatedJson, openapi::Problem,
        state::AppState,
    },
    modules::auth::{
        dto::{ForgotPasswordRequest, ResetPasswordRequest},
        service,
    },
};

#[utoipa::path(
    post,
    path = "/auth/password/forgot",
    tag = "auth",
    summary = "Envia um código de recuperação de senha",
    description = "Sempre devolve 204, exista ou não a conta, para não permitir \
                   enumeração de e-mails. O código vale 30 minutos e só pode ser \
                   usado uma vez.",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 204, description = "Pedido recebido"),
        (status = 400, description = "JSON inválido", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
        (status = 429, description = "Muitos pedidos", body = Problem),
    )
)]
pub(super) async fn forgot(
    State(state): State<AppState>,
    client_ip: ClientIp,
    ValidatedJson(body): ValidatedJson<ForgotPasswordRequest>,
) -> Result<StatusCode, AppError> {
    service::request_reset(&state, body, client_ip.as_str()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/auth/password/reset",
    tag = "auth",
    summary = "Define a nova senha com o código recebido",
    description = "O código é consumido e todas as sessões do usuário são \
                   encerradas. Contas com e-mail desativadas seguem bloqueadas.",
    request_body = ResetPasswordRequest,
    responses(
        (status = 204, description = "Senha redefinida; faça login"),
        (status = 400, description = "Código inválido, expirado ou senha fraca", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
    )
)]
pub(super) async fn reset(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<ResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    service::confirm_reset(&state, body).await?;
    Ok(StatusCode::NO_CONTENT)
}
