//! Cookie do refresh token.
//!
//! O access token fica só na memória do cliente; o refresh é `HttpOnly` e
//! limitado ao caminho do auth para não acompanhar as demais rotas da API.

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::config::settings::Settings;

/// Nome do cookie de refresh.
pub const REFRESH_COOKIE: &str = "refresh_token";

/// Só login, refresh e logout recebem o cookie.
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";

/// Lê o refresh token do cookie, sem decodificar nem validar.
pub fn get(jar: &CookieJar) -> Option<String> {
    jar.get(REFRESH_COOKIE)
        .map(|cookie| cookie.value().to_owned())
}

/// Grava o refresh token com as proteções configuradas.
pub fn set(jar: CookieJar, settings: &Settings, token: &str) -> CookieJar {
    let cookie = Cookie::build((REFRESH_COOKIE, token.to_owned()))
        .http_only(true)
        .secure(settings.security.cookie_secure)
        .same_site(same_site(&settings.security.cookie_same_site))
        .path(REFRESH_COOKIE_PATH)
        .max_age(settings.jwt.refresh_ttl().try_into().unwrap_or_default())
        .build();

    jar.add(cookie)
}

/// Remove o cookie; o navegador deixa de enviá-lo na próxima requisição.
pub fn clear(jar: CookieJar, settings: &Settings) -> CookieJar {
    let cookie = Cookie::build((REFRESH_COOKIE, ""))
        .http_only(true)
        .secure(settings.security.cookie_secure)
        .same_site(same_site(&settings.security.cookie_same_site))
        .path(REFRESH_COOKIE_PATH)
        .build();

    jar.remove(cookie)
}

fn same_site(value: &str) -> SameSite {
    match value {
        "none" => SameSite::None,
        "lax" => SameSite::Lax,
        _ => SameSite::Strict,
    }
}
