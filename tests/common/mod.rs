//! Infra compartilhada dos testes de integração.
//!
//! Com `cargo nextest`, o setup script (`scripts/test-infra.sh`) sobe um
//! Postgres/Redis por execução e publica `TEST_DATABASE_URL`/`TEST_REDIS_URL`.
//! Cada `Harness` recebe um banco próprio no Postgres e um DB lógico próprio no
//! Redis, então os testes continuam isolados e podem rodar em paralelo.
//!
//! Sem essas variáveis (ex.: `cargo test`), cai no testcontainers: cada teste
//! sobe seu par de containers. O fallback também mantém os testes rodando em
//! ambientes em que o setup script não é executado.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::header::SET_COOKIE;
use axum_test::{TestResponse, TestServer};
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder,
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
    database::connection,
    database::entity::email_outbox::{Column as OutboxColumn, Entity as Outbox},
    migrations::Migrator,
    modules::{
        audit::service::Actor,
        users::service::{self as users_service, NewUser},
    },
};
use testcontainers::{ContainerAsync, Image, ImageExt, runners::AsyncRunner};
use testcontainers_modules::{postgres::Postgres, redis::Redis};
use uuid::Uuid;

pub const ADMIN_EMAIL: &str = "admin@teste.local";
pub const ADMIN_PASSWORD: &str = "AdminTeste#2026";

/// DBs lógicos do Redis de teste; o DB 0 fica com o contador da alocação.
const REDIS_DATABASES: i64 = 64;
const PORT_ATTEMPTS: u32 = 10;

struct Infra {
    postgres_url: String,
    redis_url: String,
    containers: Option<(ContainerAsync<Postgres>, ContainerAsync<Redis>)>,
}

impl Infra {
    async fn start() -> Self {
        match (
            std::env::var("TEST_DATABASE_URL"),
            std::env::var("TEST_REDIS_URL"),
        ) {
            (Ok(postgres_url), Ok(redis_url))
                if !postgres_url.is_empty() && !redis_url.is_empty() =>
            {
                Self {
                    postgres_url,
                    redis_url,
                    containers: None,
                }
            }
            _ => Self::start_testcontainers().await,
        }
    }

    async fn start_testcontainers() -> Self {
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

        Self {
            postgres_url: format!(
                "postgres://postgres:postgres@127.0.0.1:{}/postgres",
                host_port(&postgres, 5432).await
            ),
            redis_url: format!("redis://127.0.0.1:{}", host_port(&redis, 6379).await),
            containers: Some((postgres, redis)),
        }
    }

    /// Infra de execução inteira (setup script), não containers por teste.
    fn shared(&self) -> bool {
        self.containers.is_none()
    }
}

/// `get_host_port_ipv4` pode falhar logo após o start do container; tenta de
/// novo em vez de derrubar o teste com `PortNotExposed`.
async fn host_port<I: Image>(container: &ContainerAsync<I>, port: u16) -> u16 {
    for attempt in 1..=PORT_ATTEMPTS {
        match container.get_host_port_ipv4(port).await {
            Ok(value) => return value,
            Err(error) if attempt == PORT_ATTEMPTS => {
                panic!("porta {port} do container não foi exposta: {error}");
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }

    unreachable!("o laço retorna ou entra em pânico")
}

/// Banco exclusivo do harness; some junto com o teste.
struct DatabaseHandle {
    admin_url: String,
    name: String,
    shared: bool,
}

impl Drop for DatabaseHandle {
    fn drop(&mut self) {
        if !self.shared {
            // O container do testcontainers é removido inteiro.
            return;
        }

        let admin_url = self.admin_url.clone();
        let name = self.name.clone();

        // `Drop` é síncrono: a thread com runtime próprio garante o cleanup
        // antes de o processo do teste encerrar.
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            runtime.block_on(async move {
                if let Ok(admin) = Database::connect(&admin_url).await {
                    let sql = format!(r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#);
                    let _ = admin.execute_unprepared(&sql).await;
                }
            });
        })
        .join();
    }
}

pub struct Harness {
    pub db: DatabaseConnection,
    pub cache: Cache,
    pub server: TestServer,
    pub settings: Settings,
    _database: DatabaseHandle,
    // Por último: containers só caem depois de pools e cleanup.
    _infra: Infra,
}

impl Harness {
    /// Sobe infra, aplica migrations, cria o admin e monta a aplicação.
    pub async fn spawn() -> Self {
        Self::spawn_with(|_| {}).await
    }

    /// Igual a `spawn`, permitindo ajustar a configuração antes de montar.
    pub async fn spawn_with(configure: impl FnOnce(&mut Settings)) -> Self {
        let infra = Infra::start().await;
        let database_name = format!("it_{}", Uuid::new_v4().simple());

        let admin = Database::connect(&infra.postgres_url)
            .await
            .expect("conexão administrativa do postgres");
        admin
            .execute_unprepared(&format!(r#"CREATE DATABASE "{database_name}""#))
            .await
            .expect("criar banco do teste");
        drop(admin);

        let mut settings = Settings::default();
        settings.database.url = postgres_with_database(&infra.postgres_url, &database_name);
        settings.redis.url = isolated_redis(&infra).await;
        settings.jwt.secret = "segredo-de-integracao-com-mais-de-32-chars".to_owned();
        configure(&mut settings);
        settings.validate().expect("configuração de teste");

        let cache = Cache::connect(&settings.redis).expect("cache");
        let db = connection::connect_api(&settings.database)
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
        let settings_for_tests = settings.clone();
        let server = TestServer::new(app::build(AppState::new(
            db.clone(),
            cache.clone(),
            jwt,
            mailer,
            Arc::new(settings),
        )));

        Self {
            db,
            cache,
            server,
            settings: settings_for_tests,
            _database: DatabaseHandle {
                admin_url: infra.postgres_url.clone(),
                name: database_name,
                shared: infra.shared(),
            },
            _infra: infra,
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

/// URL do Postgres apontando para outro banco, preservando a query string.
fn postgres_with_database(url: &str, database: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let prefix = base
        .rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .unwrap_or(base);
    let suffix = query.map(|query| format!("?{query}")).unwrap_or_default();

    format!("{prefix}/{database}{suffix}")
}

/// Aloca um DB lógico por harness no Redis compartilhado.
///
/// O contador fica no DB 0 e o `FLUSHDB` limpa sobras de execuções anteriores.
async fn isolated_redis(infra: &Infra) -> String {
    if !infra.shared() {
        return infra.redis_url.clone();
    }

    let client = redis::Client::open(infra.redis_url.clone()).expect("cliente redis");
    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .expect("conexão com o redis");

    let counter: i64 = redis::cmd("INCR")
        .arg("test:redis:db-index")
        .query_async(&mut connection)
        .await
        .expect("alocar DB lógico do redis");
    let index = 1 + (counter - 1).rem_euclid(REDIS_DATABASES - 1);

    let url = redis_with_database(&infra.redis_url, index);
    let isolated = redis::Client::open(url.clone()).expect("cliente redis isolado");
    let mut connection = isolated
        .get_multiplexed_async_connection()
        .await
        .expect("conexão com o DB lógico");
    redis::cmd("FLUSHDB")
        .query_async::<()>(&mut connection)
        .await
        .expect("TEST_REDIS_URL precisa aceitar SELECT (use --databases 64)");

    url
}

/// Troca (ou acrescenta) o índice de DB lógico em uma URL Redis.
fn redis_with_database(url: &str, index: i64) -> String {
    match url.rsplit_once('/') {
        Some((prefix, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => {
            format!("{prefix}/{index}")
        }
        _ => format!("{}/{index}", url.trim_end_matches('/')),
    }
}

/// Login completo, devolvendo `accessToken` e o refresh lido do cookie.
///
/// O corpo real só tem o access token; o `refreshToken` injetado aqui simula o
/// que o navegador guardaria no cookie `HttpOnly` para os testes.
pub async fn session(server: &TestServer, email: &str, password: &str) -> Value {
    let response = server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await;

    session_from_response(response)
}

pub fn session_from_response(response: TestResponse) -> Value {
    response.assert_status_ok();
    let refresh_token = refresh_token_from(&response);
    let mut body: Value = response.json();
    body["refreshToken"] = Value::String(refresh_token);
    body
}

/// Valor do cookie de refresh devolvido em `Set-Cookie`.
pub fn refresh_token_from(response: &TestResponse) -> String {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let pair = value.split(';').next().unwrap_or(value);
            pair.strip_prefix("refresh_token=").map(str::to_owned)
        })
        .filter(|value| !value.is_empty())
        .expect("Set-Cookie de refresh")
}

pub fn refresh_cookie(token: &str) -> String {
    format!("refresh_token={token}")
}

/// Refresh usando o cookie, como o navegador faria.
pub async fn refresh(server: &TestServer, token: &str) -> TestResponse {
    server
        .post("/api/v1/auth/refresh")
        .add_header("cookie", refresh_cookie(token))
        .await
}

/// Logout usando o cookie.
pub async fn logout(server: &TestServer, token: &str) -> TestResponse {
    server
        .post("/api/v1/auth/logout")
        .add_header("cookie", refresh_cookie(token))
        .await
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
