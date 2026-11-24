//! Fluxo de ponta a ponta da gestão de usuários, com Postgres e Redis reais.
//!
//! Um único teste mantém os containers vivos e executa o fluxo completo em
//! ordem. Os testes unitários cobrem as regras puras (último admin etc.).

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum_test::TestServer;
use sea_orm::{ColumnTrait, Database, EntityTrait, PaginatorTrait, QueryFilter};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use template_backend_rust::{
    app,
    config::settings::Settings,
    core::auth::role::Role,
    core::{
        auth::jwt::JwtService,
        cache::Cache,
        mail::{self, EmailMessage, Mailer, MailerError},
        state::AppState,
    },
    database::entity::email_outbox::{Column as OutboxColumn, Entity as Outbox},
    migrations::Migrator,
    modules::{
        email::service as email_service,
        users::service::{self as users_service, NewUser},
    },
};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::{postgres::Postgres, redis::Redis};

const ADMIN_EMAIL: &str = "admin@teste.local";
const ADMIN_PASSWORD: &str = "AdminTeste#2026";

#[tokio::test]
async fn fluxo_completo_de_usuarios() {
    // --- infra ---
    let postgres = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .expect("postgres");
    let redis = Redis::default()
        .with_tag("8-alpine")
        .start()
        .await
        .expect("redis");

    let pg_port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("porta do postgres");
    let redis_port = redis
        .get_host_port_ipv4(6379)
        .await
        .expect("porta do redis");

    let mut settings = Settings::default();
    settings.database.url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");
    settings.redis.url = format!("redis://127.0.0.1:{redis_port}");
    settings.jwt.secret = "segredo-de-integracao-com-mais-de-32-chars".to_owned();
    settings.validate().expect("configuração de teste");

    let db = Database::connect(&settings.database.url)
        .await
        .expect("conexão com o postgres");
    Migrator::up(&db, None).await.expect("migrations");
    let db_check = db.clone();

    users_service::create(
        &db,
        NewUser {
            name: "Admin".to_owned(),
            email: ADMIN_EMAIL.to_owned(),
            password: ADMIN_PASSWORD.to_owned(),
            role: Role::Admin,
        },
    )
    .await
    .expect("admin de teste");

    let cache = Cache::connect(&settings.redis).expect("cache");
    let jwt = JwtService::new(&settings.jwt).expect("jwt");
    let mailer = mail::from_settings(&settings.mail).expect("mailer");
    let server = TestServer::new(app::build(AppState::new(
        db,
        cache,
        jwt,
        mailer,
        Arc::new(settings),
    )));

    // --- sem token ---
    let response = server.get("/api/v1/users").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert!(
        response
            .content_type()
            .starts_with("application/problem+json")
    );

    let admin = login(&server, ADMIN_EMAIL, ADMIN_PASSWORD).await;

    // --- criação ---
    let response = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Ana",
            "email": "ana@teste.local",
            "password": "SenhaDaAna#2026",
            "role": "user"
        }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: Value = response.json();
    assert_eq!(created["email"], "ana@teste.local");
    assert_eq!(created["role"], "user");
    assert!(created.get("passwordHash").is_none());
    let ana_id = created["id"].as_str().expect("id da ana").to_owned();

    let duplicate = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Outra",
            "email": "ANA@teste.local",
            "password": "OutraSenha#2026"
        }))
        .await;
    duplicate.assert_status(StatusCode::CONFLICT);

    let invalid = server
        .post("/api/v1/users")
        .add_header("authorization", bearer(&admin))
        .json(&json!({
            "name": "Curta",
            "email": "curta@teste.local",
            "password": "curta"
        }))
        .await;
    invalid.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        invalid
            .content_type()
            .starts_with("application/problem+json")
    );

    // --- listagem e filtro ---
    let page: Value = server
        .get("/api/v1/users?search=ana&perPage=10")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], ana_id.as_str());
    assert_eq!(page["perPage"], 10);

    // --- usuário comum não gerencia usuários ---
    let ana = login(&server, "ana@teste.local", "SenhaDaAna#2026").await;
    let forbidden = server
        .get("/api/v1/users")
        .add_header("authorization", bearer(&ana))
        .await;
    forbidden.assert_status(StatusCode::FORBIDDEN);
    assert!(
        forbidden
            .content_type()
            .starts_with("application/problem+json")
    );

    // --- mudança de papel invalida a sessão antiga ---
    let promoted = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "name": "Ana Silva", "role": "admin" }))
        .await;
    promoted.assert_status_ok();
    let promoted: Value = promoted.json();
    assert_eq!(promoted["name"], "Ana Silva");
    assert_eq!(promoted["role"], "admin");

    let stale = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana))
        .await;
    stale.assert_status(StatusCode::UNAUTHORIZED);

    let ana_admin = login(&server, "ana@teste.local", "SenhaDaAna#2026").await;
    let me: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_admin))
        .await
        .json();
    assert_eq!(me["role"], "admin");

    // --- rebaixar (há outro admin) e desativar ---
    let demoted = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "role": "user" }))
        .await;
    demoted.assert_status_ok();

    let deleted = server
        .delete(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    deleted.assert_status_no_content();

    // Soft delete é idempotente.
    let deleted_again = server
        .delete(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    deleted_again.assert_status_no_content();

    let inactive: Value = server
        .get(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    assert_eq!(inactive["active"], false);

    // --- redefinir senha e reativar ---
    let password = server
        .put(&format!("/api/v1/users/{ana_id}/password"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "password": "SenhaNova#2026" }))
        .await;
    password.assert_status_no_content();

    let reactivated = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({ "active": true }))
        .await;
    reactivated.assert_status_ok();

    let old_login = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaDaAna#2026" }))
        .await;
    old_login.assert_status(StatusCode::UNAUTHORIZED);

    let new_login = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaNova#2026" }))
        .await;
    new_login.assert_status_ok();
    let ana_session: Value = new_login.json();
    let ana_token = ana_session["accessToken"]
        .as_str()
        .expect("access token da ana")
        .to_owned();
    let ana_refresh = ana_session["refreshToken"]
        .as_str()
        .expect("refresh token da ana")
        .to_owned();

    // --- perfil do próprio usuário ---
    let patched: Value = server
        .patch("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({ "name": "Ana Souza" }))
        .await
        .json();
    assert_eq!(patched["name"], "Ana Souza");

    let me_ana: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .await
        .json();
    assert_eq!(me_ana["name"], "Ana Souza");

    let short = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "curta"
        }))
        .await;
    short.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let wrong_current = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "senha-errada#2026",
            "newPassword": "SenhaFinal#2026"
        }))
        .await;
    wrong_current.assert_status(StatusCode::UNAUTHORIZED);

    let same_password = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "SenhaNova#2026"
        }))
        .await;
    same_password.assert_status(StatusCode::BAD_REQUEST);

    let changed = server
        .put("/api/v1/auth/me/password")
        .add_header("authorization", bearer(&ana_token))
        .json(&json!({
            "currentPassword": "SenhaNova#2026",
            "newPassword": "SenhaFinal#2026"
        }))
        .await;
    changed.assert_status_no_content();

    // A troca derruba inclusive a sessão que a fez.
    let stale_after_change = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&ana_token))
        .await;
    stale_after_change.assert_status(StatusCode::UNAUTHORIZED);

    let stale_refresh = server
        .post("/api/v1/auth/refresh")
        .json(&json!({ "refreshToken": ana_refresh }))
        .await;
    stale_refresh.assert_status(StatusCode::UNAUTHORIZED);

    let old_password = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaNova#2026" }))
        .await;
    old_password.assert_status(StatusCode::UNAUTHORIZED);

    let final_login = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "ana@teste.local", "password": "SenhaFinal#2026" }))
        .await;
    final_login.assert_status_ok();

    // --- proteções do próprio admin ---
    let admin_me: Value = server
        .get("/api/v1/auth/me")
        .add_header("authorization", bearer(&admin))
        .await
        .json();
    let admin_id = admin_me["id"].as_str().expect("id do admin");

    let self_delete = server
        .delete(&format!("/api/v1/users/{admin_id}"))
        .add_header("authorization", bearer(&admin))
        .await;
    self_delete.assert_status(StatusCode::CONFLICT);

    let empty_patch = server
        .patch(&format!("/api/v1/users/{ana_id}"))
        .add_header("authorization", bearer(&admin))
        .json(&json!({}))
        .await;
    empty_patch.assert_status(StatusCode::BAD_REQUEST);

    // --- outbox de e-mail ---
    let pending = Outbox::find()
        .filter(OutboxColumn::Status.eq("pending"))
        .count(&db_check)
        .await
        .expect("contagem do outbox");
    assert_eq!(pending, 4, "admin + ana + reset + troca própria");

    let recording = RecordingMailer::default();
    let summary = email_service::process_pending(&db_check, &recording, 10)
        .await
        .expect("processar outbox");
    assert_eq!(summary.claimed, 4);
    assert_eq!(summary.sent, 4);
    assert_eq!(summary.failed, 0);

    let subjects: Vec<String> = recording
        .sent
        .lock()
        .expect("lock")
        .iter()
        .map(|message| message.subject.clone())
        .collect();
    assert_eq!(
        subjects
            .iter()
            .filter(|subject| *subject == "Sua conta foi criada")
            .count(),
        2
    );
    assert_eq!(
        subjects
            .iter()
            .filter(|subject| *subject == "Sua senha foi alterada")
            .count(),
        2
    );

    let sent = Outbox::find()
        .filter(OutboxColumn::Status.eq("sent"))
        .count(&db_check)
        .await
        .expect("enviados");
    assert_eq!(sent, 4);

    // Deduplicação pelo dedupe_key.
    let message = EmailMessage {
        to: "dedupe@teste.local".to_owned(),
        subject: "Teste".to_owned(),
        body: "corpo".to_owned(),
    };
    email_service::enqueue(&db_check, message.clone(), "manual-dedupe".to_owned())
        .await
        .expect("primeiro enqueue");
    email_service::enqueue(&db_check, message, "manual-dedupe".to_owned())
        .await
        .expect("segundo enqueue");

    let deduped = Outbox::find()
        .filter(OutboxColumn::DedupeKey.eq("manual-dedupe"))
        .count(&db_check)
        .await
        .expect("dedupe");
    assert_eq!(deduped, 1);

    // Falha de envio reagenda com backoff e incrementa attempts.
    let summary = email_service::process_pending(&db_check, &FailingMailer, 10)
        .await
        .expect("processar com falha");
    assert_eq!(summary.claimed, 1);
    assert_eq!(summary.retried, 1);

    let manual = Outbox::find()
        .filter(OutboxColumn::DedupeKey.eq("manual-dedupe"))
        .one(&db_check)
        .await
        .expect("buscar manual")
        .expect("manual existe");
    assert_eq!(manual.status, "pending");
    assert_eq!(manual.attempts, 1);
    assert!(manual.last_error.is_some());

    // --- documentação ---
    let spec: Value = server.get("/api-docs/openapi.json").await.json();
    assert!(spec["paths"]["/api/v1/users"].is_object());
    assert!(spec["paths"]["/api/v1/auth/me"]["patch"].is_object());
    assert!(spec["paths"]["/api/v1/auth/me/password"]["put"].is_object());
    assert!(spec["components"]["securitySchemes"]["bearer"].is_object());

    let docs = server.get("/docs").await;
    docs.assert_status_ok();
    assert!(docs.text().contains("Scalar"));
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn login(server: &TestServer, email: &str, password: &str) -> String {
    let response = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    body["accessToken"]
        .as_str()
        .expect("access token")
        .to_owned()
}

#[derive(Default)]
struct RecordingMailer {
    sent: Mutex<Vec<EmailMessage>>,
}

#[async_trait::async_trait]
impl Mailer for RecordingMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailerError> {
        self.sent.lock().expect("lock").push(message.clone());
        Ok(())
    }
}

struct FailingMailer;

#[async_trait::async_trait]
impl Mailer for FailingMailer {
    async fn send(&self, _message: &EmailMessage) -> Result<(), MailerError> {
        Err(MailerError::Transport("falha simulada".to_owned()))
    }
}
