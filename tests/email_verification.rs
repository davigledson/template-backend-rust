//! Confirmação de e-mail: reenvio público, troca de endereço e flag de login.

mod common;

use axum::http::StatusCode;
use common::{
    ADMIN_EMAIL, ADMIN_PASSWORD, Harness, bearer, verification_email_count, verification_token,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::json;
use template_backend_rust::{
    database::entity::audit_log::{Column as AuditColumn, Entity as Audit},
    modules::{audit::service::Actor, users::service as users_service},
};

#[tokio::test]
async fn reenvio_publico_e_troca_de_email_invalidam_codigo() {
    let app = Harness::spawn().await;
    let server = &app.server;

    // --- reenvio sem sessão ---
    app.create_user("Ana", "ana@teste.local", "SenhaDaAna#2026")
        .await;
    assert_eq!(
        verification_email_count(&app.db, "ana@teste.local").await,
        1
    );

    let resend = server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "ana@teste.local" }))
        .await;
    resend.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        verification_email_count(&app.db, "ana@teste.local").await,
        2
    );

    // Conta inexistente também devolve 204 e não enfileira nada.
    let unknown = server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "ninguem@teste.local" }))
        .await;
    unknown.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        verification_email_count(&app.db, "ninguem@teste.local").await,
        0
    );

    // O limite é por conta: o quarto pedido da Ana na janela é bloqueado.
    for _ in 0..2 {
        server
            .post("/api/v1/auth/email/resend")
            .json(&json!({ "email": "ana@teste.local" }))
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }

    let blocked = server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "ana@teste.local" }))
        .await;
    blocked.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert!(blocked.headers().get("retry-after").is_some());

    // --- troca de e-mail invalida o código pendente ---
    let bia = app
        .create_user("Bia", "bia@teste.local", "SenhaDaBia#2026")
        .await;
    let bia_id = bia["id"].as_str().expect("id da Bia");
    let old_token = verification_token(&app.db, "bia@teste.local").await;

    let admin = app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
    let updated = server
        .patch(&format!("/api/v1/users/{bia_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "email": "bia.nova@teste.local" }))
        .await;
    updated.assert_status_ok();

    let new_token = verification_token(&app.db, "bia.nova@teste.local").await;
    assert_ne!(old_token, new_token);

    let stale = server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": old_token }))
        .await;
    stale.assert_status(StatusCode::BAD_REQUEST);

    let fresh = server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": new_token }))
        .await;
    fresh.assert_status(StatusCode::NO_CONTENT);

    let bia = users_service::find_by_email(&app.db, "bia.nova@teste.local")
        .await
        .expect("buscar Bia")
        .expect("Bia existe");
    assert!(bia.email_verified_at.is_some());

    // --- confirmação manual (CLI) é idempotente e auditada ---
    let caio = app
        .create_user("Caio", "caio@teste.local", "SenhaDoCaio#2026")
        .await;
    let caio_id = caio["id"]
        .as_str()
        .expect("id do Caio")
        .parse()
        .expect("uuid do Caio");

    assert!(
        users_service::verify_email(&app.db, Actor::system(), caio_id)
            .await
            .expect("confirmar Caio")
    );
    assert!(
        !users_service::verify_email(&app.db, Actor::system(), caio_id)
            .await
            .expect("repetir confirmação")
    );

    let audited = Audit::find()
        .filter(AuditColumn::Action.eq("user.email_verified"))
        .count(&app.db)
        .await
        .expect("auditoria");
    assert_eq!(audited, 1);
}

#[tokio::test]
async fn login_exige_email_confirmado_quando_flag_ligada() {
    let app = Harness::spawn_with(|settings| {
        settings.security.require_email_verified = true;
        settings.mail.provider = "smtp".to_owned();
        settings.mail.smtp_url = "smtp://127.0.0.1:1025".to_owned();
    })
    .await;

    app.create_user("Carla", "carla@teste.local", "SenhaDaCarla#2026")
        .await;

    let blocked = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": "carla@teste.local",
            "password": "SenhaDaCarla#2026"
        }))
        .await;
    blocked.assert_status(StatusCode::FORBIDDEN);

    let resend = app
        .server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "carla@teste.local" }))
        .await;
    resend.assert_status(StatusCode::NO_CONTENT);

    let token = verification_token(&app.db, "carla@teste.local").await;
    let verified = app
        .server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": token }))
        .await;
    verified.assert_status(StatusCode::NO_CONTENT);

    let login = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({
            "email": "carla@teste.local",
            "password": "SenhaDaCarla#2026"
        }))
        .await;
    login.assert_status_ok();
}
