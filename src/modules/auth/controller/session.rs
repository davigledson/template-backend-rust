//! Sessão: login, refresh e logout.
//!
//! O access token volta no corpo; o refresh fica em cookie `HttpOnly` limitado
//! a `/api/v1/auth`. Refresh e logout não aceitam mais o token no corpo.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header::ORIGIN},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    core::{
        client_ip::ClientIp,
        error::AppError,
        extract::{UserAgent, ValidatedJson},
        openapi::Problem,
        state::AppState,
    },
    modules::auth::{
        cookie,
        dto::{LoginRequest, TokenResponse},
        service,
    },
};

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    summary = "Autentica e emite o access token",
    description = "O refresh token vai em cookie HttpOnly; o corpo tem apenas o access token.",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Access token emitido; refresh no cookie", body = TokenResponse),
        (status = 400, description = "JSON inválido", body = Problem),
        (status = 401, description = "Credenciais inválidas", body = Problem),
        (status = 403, description = "Origem não permitida", body = Problem),
        (status = 422, description = "Validação falhou", body = Problem),
        (status = 429, description = "Muitas tentativas de login", body = Problem),
    )
)]
pub(super) async fn login(
    State(state): State<AppState>,
    client_ip: ClientIp,
    user_agent: UserAgent,
    headers: HeaderMap,
    jar: CookieJar,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    ensure_allowed_origin(&state, &headers)?;

    let issued = service::login(&state, body, client_ip.as_str(), user_agent.0.as_deref()).await?;
    let jar = cookie::set(jar, &state.config, &issued.refresh_token);

    Ok((jar, Json(TokenResponse::from(issued))))
}

#[utoipa::path(
    post,
    path = "/auth/refresh",
    tag = "auth",
    summary = "Rotaciona o refresh token do cookie",
    description = "O refresh token usado é revogado; um novo par é emitido e o cookie é renovado.",
    responses(
        (status = 200, description = "Novo access token; cookie de refresh rotacionado", body = TokenResponse),
        (status = 401, description = "Refresh token inválido ou expirado", body = Problem),
        (status = 403, description = "Origem não permitida", body = Problem),
    )
)]
pub(super) async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    if let Err(error) = ensure_allowed_origin(&state, &headers) {
        return error.into_response();
    }

    let Some(token) = cookie::get(&jar) else {
        let jar = cookie::clear(jar, &state.config);
        return (jar, invalid_refresh()).into_response();
    };

    match service::refresh(&state, &token).await {
        Ok(issued) => {
            let jar = cookie::set(jar, &state.config, &issued.refresh_token);
            (jar, Json(TokenResponse::from(issued))).into_response()
        }
        Err(error) => {
            let jar = if error.status() == StatusCode::UNAUTHORIZED {
                cookie::clear(jar, &state.config)
            } else {
                jar
            };
            (jar, error).into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    summary = "Revoga o refresh token do cookie",
    description = "Revoga a sessão do cookie e o remove da resposta.",
    responses(
        (status = 204, description = "Sessão encerrada"),
        (status = 403, description = "Origem não permitida", body = Problem),
    )
)]
pub(super) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    if let Err(error) = ensure_allowed_origin(&state, &headers) {
        return error.into_response();
    }

    let token = cookie::get(&jar);
    if let Err(error) = service::logout(&state, token.as_deref()).await {
        return error.into_response();
    }

    let jar = cookie::clear(jar, &state.config);
    (StatusCode::NO_CONTENT, jar).into_response()
}

/// Defesa em profundidade contra CSRF quando o front está em outro site.
///
/// Sem `Origin` (curl, app nativo) a checagem não se aplica. Com `Origin`, a
/// requisição só passa se for do mesmo host da API ou de uma origem listada no
/// CORS. Isso fecha também login CSRF em deployment same-origin.
fn ensure_allowed_origin(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };

    if origin_matches_host(origin, headers)
        || state
            .config
            .cors
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
    {
        Ok(())
    } else {
        Err(AppError::Forbidden("origem não permitida".to_owned()))
    }
}

fn origin_matches_host(origin: &str, headers: &HeaderMap) -> bool {
    let Some(host) = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    let authority = origin
        .split_once("://")
        .map_or(origin, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or(origin);

    authority.eq_ignore_ascii_case(host)
}

fn invalid_refresh() -> AppError {
    AppError::Unauthorized("refresh token inválido ou expirado".to_owned())
}
