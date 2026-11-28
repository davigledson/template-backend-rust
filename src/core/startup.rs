//! Banner de desenvolvimento com os atalhos da API.
//!
//! Só o perfil `development` imprime, depois do bind da porta. Em terminal,
//! cada URL vira hiperlink OSC 8; quando a saída é um arquivo ou o log do hub,
//! sai como texto puro, sem códigos de escape.

use std::{
    fmt::Write as _,
    io::{IsTerminal, Write as _},
    net::SocketAddr,
};

use crate::config::Settings;

const LABEL_WIDTH: usize = 8;

/// Imprime os atalhos apenas no perfil `development`.
pub fn print_if_development(settings: &Settings, address: SocketAddr) {
    if !is_development(&settings.profile) {
        return;
    }

    let hyperlinks = std::io::stdout().is_terminal();
    let banner = render(settings, address, hyperlinks);
    // Um pipe fechado não deve derrubar a API por causa do banner.
    let _ = std::io::stdout().write_all(banner.as_bytes());
}

fn is_development(profile: &str) -> bool {
    profile == "development"
}

/// Monta o bloco de atalhos. Separado do stdout para ser testável.
fn render(settings: &Settings, address: SocketAddr, hyperlinks: bool) -> String {
    let base = format!(
        "http://{}:{}",
        display_host(&settings.server.host),
        address.port()
    );
    let mut out = String::new();

    let _ = writeln!(out, "api {} ({})", crate::APP_VERSION, settings.profile);
    let _ = writeln!(out);

    row(&mut out, "API", &format!("{base}/api/v1"), hyperlinks, "");
    if settings.docs.enabled {
        row(&mut out, "UI", &format!("{base}/docs"), hyperlinks, "");
        row(
            &mut out,
            "JSON",
            &format!("{base}/api-docs/openapi.json"),
            hyperlinks,
            "",
        );
    }
    row(
        &mut out,
        "Health",
        &format!("{base}/health"),
        hyperlinks,
        "",
    );
    row(
        &mut out,
        "Ready",
        &format!("{base}/health/ready"),
        hyperlinks,
        "",
    );
    if settings.metrics.enabled {
        let suffix = if settings.metrics.token().is_some() {
            " (Bearer exigido)"
        } else {
            ""
        };
        row(
            &mut out,
            "Metrics",
            &format!("{base}/metrics"),
            hyperlinks,
            suffix,
        );
    }

    if let Some((first, rest)) = front_origins(settings).split_first() {
        row(&mut out, "Front", first, hyperlinks, "");
        for origin in rest {
            row(&mut out, "", origin, hyperlinks, "");
        }
    }

    let _ = writeln!(out);
    out
}

fn row(out: &mut String, label: &str, url: &str, hyperlinks: bool, suffix: &str) {
    let _ = writeln!(
        out,
        "  {label:<LABEL_WIDTH$} {}{suffix}",
        link(url, hyperlinks)
    );
}

/// Origens de CORS que apontam para um front HTTP(S), sem repetição.
fn front_origins(settings: &Settings) -> Vec<&str> {
    let mut origins: Vec<&str> = Vec::new();
    for origin in &settings.cors.allowed_origins {
        let origin = origin.trim();
        let is_http = origin.starts_with("http://") || origin.starts_with("https://");
        if is_http && !origins.contains(&origin) {
            origins.push(origin);
        }
    }
    origins
}

fn display_host(configured: &str) -> String {
    let host = configured.trim();
    if host.is_empty() || matches!(host, "0.0.0.0" | "::" | "[::]") {
        return "localhost".to_owned();
    }
    if host.contains(':') && !host.starts_with('[') {
        return format!("[{host}]");
    }
    host.to_owned()
}

/// Hiperlink OSC 8; a URL também é o texto, então continua copiável.
fn link(url: &str, hyperlinks: bool) -> String {
    if !hyperlinks {
        return url.to_owned();
    }
    format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> SocketAddr {
        "127.0.0.1:8080".parse().expect("endereço de teste")
    }

    fn settings() -> Settings {
        Settings {
            profile: "development".to_owned(),
            ..Settings::default()
        }
    }

    #[test]
    fn lista_os_atalhos_do_backend() {
        let out = render(&settings(), address(), false);

        for url in [
            "http://localhost:8080/api/v1",
            "http://localhost:8080/docs",
            "http://localhost:8080/api-docs/openapi.json",
            "http://localhost:8080/health",
            "http://localhost:8080/health/ready",
            "http://localhost:8080/metrics",
        ] {
            assert!(out.contains(url), "faltou {url}:\n{out}");
        }
        assert!(out.starts_with("api "));
    }

    #[test]
    fn omite_documentacao_quando_desligada() {
        let mut settings = settings();
        settings.docs.enabled = false;

        let out = render(&settings, address(), false);

        assert!(!out.contains("/docs"));
        assert!(!out.contains("api-docs"));
        assert!(out.contains("/health"));
    }

    #[test]
    fn omite_metricas_quando_desligadas() {
        let mut settings = settings();
        settings.metrics.enabled = false;

        let out = render(&settings, address(), false);

        assert!(!out.contains("/metrics"));
    }

    #[test]
    fn avisa_bearer_sem_revelar_o_token() {
        let mut settings = settings();
        settings.metrics.token = "segredo-de-metricas-123".to_owned();

        let out = render(&settings, address(), false);

        assert!(out.contains("Bearer exigido"));
        assert!(!out.contains("segredo-de-metricas-123"));
    }

    #[test]
    fn lista_origens_do_front_sem_repetir() {
        let mut settings = settings();
        settings.cors.allowed_origins = vec![
            "http://localhost:3010".to_owned(),
            "http://localhost:3010".to_owned(),
            "http://localhost:5173".to_owned(),
            "invalido".to_owned(),
        ];

        let out = render(&settings, address(), false);

        assert_eq!(out.matches("http://localhost:3010").count(), 1);
        assert!(out.contains("http://localhost:5173"));
        assert!(!out.contains("invalido"));
        assert!(out.contains("Front"));
    }

    #[test]
    fn nao_lista_front_sem_origens() {
        let mut settings = settings();
        settings.cors.allowed_origins.clear();

        let out = render(&settings, address(), false);

        assert!(!out.contains("Front"));
    }

    #[test]
    fn mapeia_host_de_escuta_para_localhost() {
        assert_eq!(display_host("0.0.0.0"), "localhost");
        assert_eq!(display_host("::"), "localhost");
        assert_eq!(display_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(display_host("api.local"), "api.local");
        assert_eq!(display_host("::1"), "[::1]");
    }

    #[test]
    fn usa_hiperlink_apenas_quando_pedido() {
        assert_eq!(
            link("http://localhost:8080/docs", false),
            "http://localhost:8080/docs"
        );
        assert!(link("http://localhost:8080/docs", true).contains("\x1b]8;;"));
    }

    #[test]
    fn imprime_apenas_em_desenvolvimento() {
        assert!(is_development("development"));
        assert!(!is_development("production"));
        assert!(!is_development("staging"));
    }
}
