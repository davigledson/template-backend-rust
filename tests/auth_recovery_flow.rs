//! Recuperação de senha, confirmação de e-mail e sessões por dispositivo.
//!
//! Os códigos são lidos do outbox (o worker entregaria o mesmo corpo por SMTP).

mod common;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use template_backend_rust::{
    config::settings::Settings,
    database::entity::{
        email_outbox::{Column as OutboxColumn, Entity as Outbox},
        users::{Column as UserColumn, Entity as Users},
    },
};

use common::{ADMIN_EMAIL, Harness, bearer, session};

#[tokio::test]
async fn recuperacao_de_senha_por_email() {
    let app = Harness::spawn().await;
    app.create_user("Ana", "ana@teste.local", "SenhaDaAna#2026")
        .await;

    // Pedido de conta existente: 204 e e-mail com código.
    let response = app
        .server
        .post("/api/v1/auth/password/forgot")
        .json(&json!({ "email": "ana@teste.local" }))
        .await;
    response.assert_status_no_content();

    let token = code_from_outbox(&app, "Redefinição de senha").await;

    // Pedido de conta inexistente devolve o mesmo 204 e não envia nada.
    let unknown = app
        .server
        .post("/api/v1/auth/password/forgot")
        .json(&json!({ "email": "ninguem@teste.local" }))
        .await;
    unknown.assert_status_no_content();

    // Código inválido não altera a senha.
    let invalid = app
        .server
        .post("/api/v1/auth/password/reset")
        .json(&json!({ "token": "codigo-invalido", "password": "OutraSenha#2026" }))
        .await;
    invalid.assert_status_bad_request();

    let reset = app
        .server
        .post("/api/v1/auth/password/reset")
        .json(&json!({ "token": token, "password": "OutraSenha#2026" }))
        .await;
    reset.assert_status_no_content();

    // O código é de uso único.
    let replay = app
        .server
        .post("/api/v1/auth/password/reset")
        .json(&json!({ "token": token, "password": "TerceiraSenha#2026" }))
        .await;
    replay.assert_status_bad_request();

    // Senha antiga não entra; a nova entra.
    app.server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaDaAna#2026" }))
        .await
        .assert_status_unauthorized();
    app.login("ana@teste.local", "OutraSenha#2026").await;

    // Receber o código no endereço também confirma o e-mail.
    let user = Users::find()
        .filter(UserColumn::Email.eq("ana@teste.local"))
        .one(&app.db)
        .await
        .expect("busca")
        .expect("usuário");
    assert!(user.email_verified_at.is_some());
}

#[tokio::test]
async fn confirmacao_de_email_libera_o_login_quando_exigida() {
    let app = Harness::spawn_with(|settings: &mut Settings| {
        settings.security.require_email_verified = true;
        settings.mail.provider = "smtp".to_owned();
        settings.mail.smtp_url = "smtp://127.0.0.1:1025".to_owned();
    })
    .await;

    app.create_user("Bia", "bia@teste.local", "SenhaDaBia#2026")
        .await;

    // Sem confirmar, o login é barrado mesmo com a senha certa.
    let blocked = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "bia@teste.local", "password": "SenhaDaBia#2026" }))
        .await;
    blocked.assert_status_forbidden();

    let token = code_from_outbox(&app, "Confirme seu e-mail").await;
    app.server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": token }))
        .await
        .assert_status_no_content();

    // Confirmar de novo (código consumido) não é aceito.
    let replay = app
        .server
        .post("/api/v1/auth/email/verify")
        .json(&json!({ "token": token }))
        .await;
    replay.assert_status_bad_request();

    app.login("bia@teste.local", "SenhaDaBia#2026").await;

    // Já confirmado: o reenvio público é inofensivo e devolve 204.
    app.server
        .post("/api/v1/auth/email/resend")
        .json(&json!({ "email": "bia@teste.local" }))
        .await
        .assert_status_no_content();

    let user = Users::find()
        .filter(UserColumn::Email.eq("bia@teste.local"))
        .one(&app.db)
        .await
        .expect("busca")
        .expect("usuário");
    assert!(user.email_verified_at.is_some());
}

#[tokio::test]
async fn sessoes_por_dispositivo_listam_e_encerram() {
    let app = Harness::spawn().await;

    let first = session(&app.server, ADMIN_EMAIL, common::ADMIN_PASSWORD).await;
    let second = session(&app.server, ADMIN_EMAIL, common::ADMIN_PASSWORD).await;

    let access = second["accessToken"].as_str().expect("access");
    let refresh = second["refreshToken"].as_str().expect("refresh");

    let sessions: Value = app
        .server
        .get("/api/v1/auth/sessions")
        .add_header("authorization", bearer(access))
        .await
        .json();
    let items = sessions["items"].as_array().expect("itens");
    assert_eq!(items.len(), 2, "{sessions}");
    assert_eq!(
        items.iter().filter(|item| item["current"] == true).count(),
        1
    );

    let current = items
        .iter()
        .find(|item| item["current"] == true)
        .expect("sessão atual");
    let other = items
        .iter()
        .find(|item| item["current"] == false)
        .expect("outra sessão");
    let created_at = current["createdAt"].as_str().expect("createdAt").to_owned();

    // O refresh renova o par sem resetar a data de criação da sessão.
    let rotated: Value = app
        .server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": refresh }))
        .await
        .json();
    let access = rotated["accessToken"].as_str().expect("access");
    let refresh = rotated["refreshToken"].as_str().expect("refresh");

    let sessions: Value = app
        .server
        .get("/api/v1/auth/sessions")
        .add_header("authorization", bearer(access))
        .await
        .json();
    let current = sessions["items"]
        .as_array()
        .expect("itens")
        .iter()
        .find(|item| item["current"] == true)
        .expect("sessão atual");
    assert_eq!(current["createdAt"].as_str(), Some(created_at.as_str()));

    // Encerrar a sessão atual é recusado; o caminho é o logout.
    app.server
        .delete(&format!(
            "/api/v1/auth/sessions/{}",
            current["familyId"].as_str().expect("family")
        ))
        .add_header("authorization", bearer(access))
        .await
        .assert_status_bad_request();

    app.server
        .delete(&format!(
            "/api/v1/auth/sessions/{}",
            other["familyId"].as_str().expect("family")
        ))
        .add_header("authorization", bearer(access))
        .await
        .assert_status_no_content();

    // O refresh da sessão encerrada não vale mais.
    app.server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": first["refreshToken"].as_str().expect("refresh") }))
        .await
        .assert_status_unauthorized();

    // Repetir o encerramento devolve 404.
    app.server
        .delete(&format!(
            "/api/v1/auth/sessions/{}",
            other["familyId"].as_str().expect("family")
        ))
        .add_header("authorization", bearer(access))
        .await
        .assert_status_not_found();

    // Logout some com a sessão da lista.
    app.server
        .post("/api/v1/auth/logout")
        .json(&json!({ "refreshToken": refresh }))
        .await
        .assert_status_no_content();

    let sessions: Value = app
        .server
        .get("/api/v1/auth/sessions")
        .add_header("authorization", bearer(access))
        .await
        .json();
    assert_eq!(sessions["items"].as_array().expect("itens").len(), 0);
}

/// Lê o código do último e-mail com o assunto informado.
async fn code_from_outbox(app: &Harness, subject: &str) -> String {
    let row = Outbox::find()
        .filter(OutboxColumn::Subject.eq(subject))
        .all(&app.db)
        .await
        .expect("outbox");
    let body = row.last().expect("e-mail com o assunto").body.clone();

    body.lines()
        .map(str::trim)
        .find(|line| line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("código não encontrado em: {body}"))
}

#[tokio::test]
async fn codigo_de_recuperacao_morre_quando_o_email_muda() {
    use template_backend_rust::modules::{
        audit::service::Actor,
        users::service::{self as users_service, UserUpdate},
    };

    let app = Harness::spawn().await;
    let user = app
        .create_user("Caio", "caio@teste.local", "SenhaDoCaio#2026")
        .await;
    let id = user["id"].as_str().expect("id").parse().expect("uuid");

    app.server
        .post("/api/v1/auth/password/forgot")
        .json(&json!({ "email": "caio@teste.local" }))
        .await
        .assert_status_no_content();
    let token = code_from_outbox(&app, "Redefinição de senha").await;

    // O admin troca o e-mail; o código emitido para o endereço antigo morre.
    users_service::update(
        &app.db,
        &app.cache,
        Actor::system(),
        id,
        UserUpdate {
            email: Some("caio.novo@teste.local".to_owned()),
            ..UserUpdate::default()
        },
    )
    .await
    .expect("troca de e-mail");

    app.server
        .post("/api/v1/auth/password/reset")
        .json(&json!({ "token": token, "password": "OutraSenha#2026" }))
        .await
        .assert_status_bad_request();

    // E a senha original continua valendo.
    app.login("caio.novo@teste.local", "SenhaDoCaio#2026").await;
}
