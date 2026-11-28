//! IP do cliente e confiança em cabeçalhos de proxy.
//!
//! O endereço do socket vem do `ConnectInfo` (servido com
//! `into_make_service_with_connect_info`). `X-Forwarded-For`/`X-Real-IP` só são
//! lidos quando `server.trust_proxy_headers = true`, porque qualquer cliente
//! pode forjar esses cabeçalhos em uma conexão direta.

use std::{convert::Infallible, net::IpAddr, net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::request::Parts,
    middleware::Next,
    response::Response,
};

use crate::config::settings::Settings;

/// IP já resolvido pelo middleware, disponível aos handlers.
#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

impl ClientIp {
    /// Chave estável para logs e rate limit; nunca vazia.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<ClientIp>()
            .cloned()
            .unwrap_or_else(|| ClientIp("unknown".to_owned())))
    }
}

/// Middleware: grava `ClientIp` nas extensions da requisição.
pub async fn resolve(
    State(settings): State<Arc<Settings>>,
    mut request: Request,
    next: Next,
) -> Response {
    let ip = forwarded_ip(&request, settings.server.trust_proxy_headers)
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_owned());

    request.extensions_mut().insert(ClientIp(sanitize(&ip)));
    next.run(request).await
}

fn forwarded_ip(request: &Request, trusted: bool) -> Option<String> {
    if !trusted {
        return None;
    }

    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    };

    trusted_forwarded(header("x-real-ip"), header("x-forwarded-for"))
}

/// `X-Real-IP` é um valor único definido pelo proxy. `X-Forwarded-For` é uma
/// lista em que o proxy confiável acrescenta o cliente no fim; por isso vale o
/// último IP válido, e não o primeiro (que o cliente consegue forjar).
///
/// Assume um único proxy na borda. Com mais de um, configure o proxy para
/// sobrescrever `X-Real-IP` em vez de repassar a lista.
fn trusted_forwarded(real_ip: Option<&str>, forwarded_for: Option<&str>) -> Option<String> {
    real_ip
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .or_else(|| {
            forwarded_for?
                .rsplit(',')
                .filter_map(|part| part.trim().parse::<IpAddr>().ok())
                .next()
        })
        .map(|ip| ip.to_string())
}

/// Limita o tamanho e remove caracteres de controle antes de usar em chave/log.
fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();

    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitiza_caracteres_de_controle() {
        assert_eq!(sanitize("203.0.113.10\n"), "203.0.113.10");
        assert_eq!(sanitize(""), "unknown");
        assert!(sanitize(&"a".repeat(200)).len() <= 64);
    }

    #[test]
    fn x_real_ip_tem_prioridade() {
        assert_eq!(
            trusted_forwarded(Some("203.0.113.7"), Some("198.51.100.9")),
            Some("203.0.113.7".to_owned())
        );
    }

    #[test]
    fn usa_o_ultimo_ip_do_forwarded_for() {
        // O proxy confiável acrescenta o cliente no fim; o início é forjável.
        assert_eq!(
            trusted_forwarded(None, Some("10.9.9.9, 203.0.113.7")),
            Some("203.0.113.7".to_owned())
        );
    }

    #[test]
    fn ignora_forwarded_for_invalido() {
        assert_eq!(trusted_forwarded(None, Some("nao-e-ip")), None);
        assert_eq!(trusted_forwarded(Some("nao-e-ip"), None), None);
    }

    #[test]
    fn aceita_ipv6() {
        assert_eq!(
            trusted_forwarded(None, Some("2001:db8::1")),
            Some("2001:db8::1".to_owned())
        );
    }
}
