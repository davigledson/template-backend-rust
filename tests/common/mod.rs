//! Infra compartilhada dos testes de integração.
//!
//! Cada binário de teste sobe seu próprio Postgres e Redis. Container e servidor
//! vivem dentro do `Harness`; quando ele cai, os containers param.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use axum_test::TestServer;
use sea_orm::{
    ColumnTrait, Database, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use template_backend_rust::{
    app,
    config::settings::Settings,
    core::{
        auth::{jwt::JwtService, role::Role},
        cache::Cache,
        mail::{self, EmailMessage, Mailer, MailerError},
        state::AppState,
    },
    database::entity::email_outbox::{Column as OutboxColumn, Entity as Outbox},
    migrations::Migrator,
    modules::{
        audit::service::Actor,
        users::service::{self as users_service, NewUser},
    },
};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::{postgres::Postgres, redis::Redis};

pub const ADMIN_EMAIL: &str = "admin@teste.local";
pub const ADMIN_PASSWORD: &str = "AdminTeste#2026";

pub struct Harness {
    // Mantém os containers vivos durante o teste.
    _postgres: ContainerAsync<Postgres>,
    _redis: ContainerAsync<Redis>,
    pub db: DatabaseConnection,
    pub cache: Cache,
    pub server: TestServer,
}

impl Harness {
    /// Sobe infra, aplica migrations, cria o admin e monta a aplicação.
    pub async fn spawn() -> Self {
        Self::spawn_with(|_| {}).await
    }

    /// Igual a `spawn`, permitindo ajustar a configuração antes de montar.
    pub async fn spawn_with(configure: impl FnOnce(&mut Settings)) -> Self {
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
        settings.database.url =
            format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");
        settings.redis.url = format!("redis://127.0.0.1:{redis_port}");
        settings.jwt.secret = "segredo-de-integracao-com-mais-de-32-chars".to_owned();
        configure(&mut settings);
        settings.validate().expect("configuração de teste");

        let cache = Cache::connect(&settings.redis).expect("cache");
        let db = Database::connect(&settings.database.url)
            .await
            .expect("conexão com o postgres");
        Migrator::up(&db, None).await.expect("migrations");

        users_service::create(
            &db,
            &cache,
            Actor::system(),
            NewUser {
                name: "Admin".to_owned(),
                email: ADMIN_EMAIL.to_owned(),
                password: ADMIN_PASSWORD.to_owned(),
                role: Role::Admin,
            },
        )
        .await
        .expect("admin de teste");

        let jwt = JwtService::new(&settings.jwt).expect("jwt");
        let mailer = mail::from_settings(&settings.mail).expect("mailer");
        let server = TestServer::new(app::build(AppState::new(
            db.clone(),
            cache.clone(),
            jwt,
            mailer,
            Arc::new(settings),
        )));

        Self {
            _postgres: postgres,
            _redis: redis,
            db,
            cache,
            server,
        }
    }

    pub async fn login(&self, email: &str, password: &str) -> String {
        session(&self.server, email, password).await["accessToken"]
            .as_str()
            .expect("access token")
            .to_owned()
    }

    /// Cria um usuário direto pelo serviço (sem passar pela API).
    pub async fn create_user(&self, name: &str, email: &str, password: &str) -> Value {
        let model = users_service::create(
            &self.db,
            &self.cache,
            Actor::system(),
            NewUser {
                name: name.to_owned(),
                email: email.to_owned(),
                password: password.to_owned(),
                role: Role::User,
            },
        )
        .await
        .expect("usuário de teste");

        json!({ "id": model.id.to_string(), "email": model.email })
    }
}

/// Login completo, devolvendo `accessToken` e `refreshToken`.
pub async fn session(server: &TestServer, email: &str, password: &str) -> Value {
    let response = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await;
    response.assert_status_ok();
    response.json()
}

pub fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Quantidade de e-mails de confirmação já enfileirados para o endereço.
pub async fn verification_email_count(db: &DatabaseConnection, email: &str) -> u64 {
    Outbox::find()
        .filter(OutboxColumn::ToEmail.eq(email))
        .filter(OutboxColumn::Subject.eq("Confirme seu e-mail"))
        .count(db)
        .await
        .expect("contagem de confirmações")
}

/// Extrai do outbox o código de confirmação mais recente do endereço.
pub async fn verification_token(db: &DatabaseConnection, email: &str) -> String {
    let item = Outbox::find()
        .filter(OutboxColumn::ToEmail.eq(email))
        .filter(OutboxColumn::Subject.eq("Confirme seu e-mail"))
        .order_by_desc(OutboxColumn::CreatedAt)
        .one(db)
        .await
        .expect("consulta do outbox")
        .expect("e-mail de confirmação no outbox");

    item.body
        .lines()
        .map(str::trim)
        .find(|line| line.len() == 64 && line.chars().all(|char| char.is_ascii_hexdigit()))
        .expect("código de confirmação no corpo")
        .to_owned()
}

#[derive(Default)]
pub struct RecordingMailer {
    pub sent: Mutex<Vec<EmailMessage>>,
}

impl RecordingMailer {
    pub fn subjects(&self) -> Vec<String> {
        self.sent
            .lock()
            .expect("lock")
            .iter()
            .map(|message| message.subject.clone())
            .collect()
    }
}

#[async_trait::async_trait]
impl Mailer for RecordingMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailerError> {
        self.sent.lock().expect("lock").push(message.clone());
        Ok(())
    }
}

pub struct FailingMailer;

#[async_trait::async_trait]
impl Mailer for FailingMailer {
    async fn send(&self, _message: &EmailMessage) -> Result<(), MailerError> {
        Err(MailerError::Transport("falha simulada".to_owned()))
    }
}
