//! Hardening HTTP: cabeçalhos, `no-store`, documentação e `/metrics`.

mod common;

use axum::http::StatusCode;
use common::{ADMIN_EMAIL, ADMIN_PASSWORD, Harness};
use sea_orm::{ConnectionTrait, Statement};
use serde_json::{Value, json};
use template_backend_rust::{
    core::auth::rate_limit::RateLimiter, core::error::AppError, database::connection,
};

const METRICS_TOKEN: &str = "token-de-teste-123456";

fn header<'a>(response: &'a axum_test::TestResponse, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .unwrap_or_else(|| panic!("cabeçalho ausente: {name}"))
        .to_str()
        .expect("cabeçalho ASCII")
}

#[tokio::test]
async fn api_aplica_statement_timeout() {
    let app = Harness::spawn().await;

    let statement = Statement::from_string(
        app.db.get_database_backend(),
        "SELECT current_setting('statement_timeout') AS timeout",
    );
    let row = app
        .db
        .query_one_raw(statement)
        .await
        .expect("consulta")
        .expect("linha");
    let timeout: String = row.try_get("", "timeout").expect("timeout");

    assert_eq!(timeout, "5s");
}

#[tokio::test]
async fn statement_timeout_vira_servico_indisponivel() {
    let app = Harness::spawn().await;
    let mut settings = app.settings.database.clone();
    settings.statement_timeout_secs = 1;

    let db = connection::connect_api(&settings).await.expect("conexão");
    let statement = Statement::from_string(db.get_database_backend(), "SELECT pg_sleep(2)");
    let error = db.query_one_raw(statement).await.expect_err("timeout");

    assert_eq!(
        AppError::from(error).status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn cabecalhos_de_seguranca_e_metricas_com_token() {
    let app = Harness::spawn_with(|settings| {
        settings.metrics.token = METRICS_TOKEN.to_owned();
        settings.security.hsts_max_age_secs = 3600;
    })
    .await;

    let response = app.server.get("/api/v1/users").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(header(&response, "x-content-type-options"), "nosniff");
    assert_eq!(header(&response, "x-frame-options"), "DENY");
    assert_eq!(header(&response, "referrer-policy"), "no-referrer");
    assert!(header(&response, "permissions-policy").contains("camera=()"));
    assert_eq!(
        header(&response, "strict-transport-security"),
        "max-age=3600"
    );
    assert_eq!(header(&response, "cache-control"), "no-store");

    // Health também recebe os cabeçalhos, mas não precisa de no-store.
    let health = app.server.get("/health").await;
    health.assert_status_ok();
    assert_eq!(header(&health, "x-frame-options"), "DENY");

    // --- /metrics ---
    let sem_token = app.server.get("/metrics").await;
    sem_token.assert_status(StatusCode::UNAUTHORIZED);
    assert!(sem_token.headers().get("www-authenticate").is_some());

    let token_errado = app
        .server
        .get("/metrics")
        .add_header("authorization", "Bearer token-errado")
        .await;
    token_errado.assert_status(StatusCode::UNAUTHORIZED);

    let metrics = app
        .server
        .get("/metrics")
        .add_header("authorization", format!("Bearer {METRICS_TOKEN}"))
        .await;
    metrics.assert_status_ok();
    assert_eq!(header(&metrics, "cache-control"), "no-store");
    assert!(metrics.text().contains("axum_http_requests_total"));
}

#[tokio::test]
async fn documentacao_e_metricas_podem_ser_desligadas() {
    let app = Harness::spawn_with(|settings| {
        settings.docs.enabled = false;
        settings.metrics.enabled = false;
    })
    .await;

    app.server
        .get("/docs")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    app.server
        .get("/api-docs/openapi.json")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    app.server
        .get("/metrics")
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // A API continua funcionando.
    app.server.get("/health").await.assert_status_ok();
    app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
}

#[tokio::test]
async fn forwarded_for_forjado_nao_define_o_ip_do_rate_limit() {
    let app = Harness::spawn_with(|settings| {
        settings.server.trust_proxy_headers = true;
    })
    .await;

    // O cliente forja o primeiro valor; o proxy confiável acrescenta o real no fim.
    app.server
        .post("/api/v1/auth/login")
        .add_header("x-forwarded-for", "10.9.9.9, 203.0.113.9")
        .json(&json!({
            "email": "nao-existe@example.com",
            "password": "senha-errada-123",
        }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    let mut connection = app.cache.pool().get().await.expect("conexão redis");
    let forged: i64 = redis::cmd("EXISTS")
        .arg(RateLimiter::ip_key("10.9.9.9"))
        .query_async(&mut connection)
        .await
        .expect("consulta redis");
    let real: i64 = redis::cmd("EXISTS")
        .arg(RateLimiter::ip_key("203.0.113.9"))
        .query_async(&mut connection)
        .await
        .expect("consulta redis");

    assert_eq!(forged, 0);
    assert_eq!(real, 1);
}

#[tokio::test]
async fn login_entrega_refresh_em_cookie_http_only() {
    let app = Harness::spawn().await;

    let response = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": ADMIN_EMAIL,
            "password": ADMIN_PASSWORD,
        }))
        .await;
    response.assert_status_ok();

    let cookie = header(&response, "set-cookie");
    assert!(cookie.starts_with("refresh_token="), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("Secure"), "{cookie}");
    assert!(cookie.contains("SameSite=Strict"), "{cookie}");
    assert!(cookie.contains("Path=/api/v1/auth"), "{cookie}");
    assert!(cookie.contains("Max-Age=604800"), "{cookie}");

    let body: Value = response.json();
    assert!(body.get("refreshToken").is_none(), "{body}");
    assert!(body["accessToken"].as_str().is_some(), "{body}");
}

#[tokio::test]
async fn refresh_e_logout_usam_o_cookie() {
    let app = Harness::spawn().await;

    app.server
        .post("/api/v1/auth/refresh")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    let login = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": ADMIN_EMAIL,
            "password": ADMIN_PASSWORD,
        }))
        .await;
    login.assert_status_ok();
    let refresh_token = common::refresh_token_from(&login);

    let rotated = common::refresh(&app.server, &refresh_token).await;
    rotated.assert_status_ok();
    let rotated_token = common::refresh_token_from(&rotated);
    assert_ne!(refresh_token, rotated_token);

    let body: Value = rotated.json();
    assert!(body.get("refreshToken").is_none(), "{body}");

    let logout = common::logout(&app.server, &rotated_token).await;
    logout.assert_status_no_content();
    let cleared = header(&logout, "set-cookie");
    assert!(cleared.starts_with("refresh_token="), "{cleared}");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");

    common::refresh(&app.server, &rotated_token)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_recusa_origem_fora_do_cors() {
    let app = Harness::spawn_with(|settings| {
        settings.cors.allowed_origins = vec!["http://app.test".to_owned()];
    })
    .await;

    let login = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": ADMIN_EMAIL,
            "password": ADMIN_PASSWORD,
        }))
        .await;
    login.assert_status_ok();
    let refresh_token = common::refresh_token_from(&login);

    app.server
        .post("/api/v1/auth/refresh")
        .add_header("cookie", common::refresh_cookie(&refresh_token))
        .add_header("origin", "http://evil.test")
        .await
        .assert_status(StatusCode::FORBIDDEN);

    app.server
        .post("/api/v1/auth/refresh")
        .add_header("cookie", common::refresh_cookie(&refresh_token))
        .add_header("origin", "http://app.test")
        .await
        .assert_status_ok();
}

#[tokio::test]
async fn origem_estranha_e_recusada_mesmo_sem_cors_configurado() {
    let app = Harness::spawn_with(|settings| {
        settings.cors.allowed_origins.clear();
    })
    .await;

    let login = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": ADMIN_EMAIL,
            "password": ADMIN_PASSWORD,
        }))
        .await;
    login.assert_status_ok();
    let refresh_token = common::refresh_token_from(&login);

    app.server
        .post("/api/v1/auth/refresh")
        .add_header("cookie", common::refresh_cookie(&refresh_token))
        .add_header("origin", "http://evil.test")
        .await
        .assert_status(StatusCode::FORBIDDEN);
}
