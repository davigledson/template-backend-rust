//! Configuração tipada, carregada em camadas.
//!
//! Precedência (do menor para o maior):
//! `default.toml` → `config/<perfil>.toml` → variáveis `APP__*` → `.env` (só dev).
//!
//! O `.env` não é uma camada do Figment: ele apenas popula variáveis de ambiente
//! quando o perfil é `development`.

use std::{fmt, path::Path, time::Duration};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use thiserror::Error;

const DEFAULT_PROFILE: &str = "development";
const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("falha ao carregar a configuração: {0}")]
    Load(Box<figment::Error>),
    #[error("configuração inválida: {0}")]
    Invalid(String),
}

impl From<figment::Error> for SettingsError {
    fn from(error: figment::Error) -> Self {
        Self::Load(Box::new(error))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Perfil ativo (`APP_PROFILE`). Não vem do TOML.
    #[serde(skip)]
    pub profile: String,
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub jwt: JwtSettings,
    pub jobs: JobSettings,
    pub log: LogSettings,
    pub cors: CorsSettings,
}

impl Settings {
    /// Carrega `default.toml`, o perfil ativo e os overrides `APP__*`.
    pub fn load() -> Result<Self, SettingsError> {
        let profile = std::env::var("APP_PROFILE").unwrap_or_else(|_| DEFAULT_PROFILE.to_owned());

        if profile == DEFAULT_PROFILE {
            let _ = dotenvy::dotenv();
        }

        let mut figment = Figment::new().merge(Toml::file("default.toml"));

        let profile_file = format!("config/{profile}.toml");
        if Path::new(&profile_file).exists() {
            figment = figment.merge(Toml::file(profile_file));
        }

        let mut settings: Self = figment
            .merge(Env::prefixed("APP__").split("__"))
            .extract()?;

        settings.profile = profile;
        settings.validate()?;
        Ok(settings)
    }

    /// Valida invariantes que não fazem sentido só no tipo.
    pub fn validate(&self) -> Result<(), SettingsError> {
        let mut problems: Vec<String> = Vec::new();

        if self.server.port == 0 {
            problems.push("server.port deve ser maior que zero".to_owned());
        }
        if self.server.request_timeout_secs == 0 {
            problems.push("server.request_timeout_secs deve ser maior que zero".to_owned());
        }
        if self.server.body_limit_mb == 0 {
            problems.push("server.body_limit_mb deve ser maior que zero".to_owned());
        }
        if self.database.url.trim().is_empty() {
            problems.push("database.url não pode ser vazia".to_owned());
        }
        if self.database.max_connections == 0 || self.database.max_connections > 100 {
            problems.push("database.max_connections deve estar entre 1 e 100".to_owned());
        }
        if self.database.min_connections > self.database.max_connections {
            problems.push("database.min_connections não pode exceder max_connections".to_owned());
        }
        if self.redis.url.trim().is_empty() {
            problems.push("redis.url não pode ser vazia".to_owned());
        }
        if self.redis.pool_size == 0 {
            problems.push("redis.pool_size deve ser maior que zero".to_owned());
        }
        if self.jwt.secret.len() < MIN_SECRET_LEN {
            problems.push(format!(
                "jwt.secret deve ter ao menos {MIN_SECRET_LEN} caracteres (defina APP__JWT__SECRET)"
            ));
        }
        if self.jwt.access_ttl_min == 0 {
            problems.push("jwt.access_ttl_min deve ser maior que zero".to_owned());
        }
        if self.jwt.refresh_ttl_days == 0 {
            problems.push("jwt.refresh_ttl_days deve ser maior que zero".to_owned());
        }
        if self.jobs.timezone.parse::<chrono_tz::Tz>().is_err() {
            problems.push(format!(
                "jobs.timezone não é um fuso IANA válido: {}",
                self.jobs.timezone
            ));
        }
        if !matches!(self.log.format.as_str(), "json" | "pretty" | "compact") {
            problems.push("log.format deve ser json, pretty ou compact".to_owned());
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(SettingsError::Invalid(problems.join("; ")))
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: DEFAULT_PROFILE.to_owned(),
            server: ServerSettings::default(),
            database: DatabaseSettings::default(),
            redis: RedisSettings::default(),
            jwt: JwtSettings::default(),
            jobs: JobSettings::default(),
            log: LogSettings::default(),
            cors: CorsSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    /// `0` deixa o Tokio escolher. Em servidor de 2 vCPU use `2`.
    pub worker_threads: usize,
    pub body_limit_mb: usize,
}

impl ServerSettings {
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    pub fn body_limit_bytes(&self) -> usize {
        self.body_limit_mb.saturating_mul(1024 * 1024)
    }

    /// `None` quando `worker_threads = 0`.
    pub fn worker_threads(&self) -> Option<usize> {
        (self.worker_threads > 0).then_some(self.worker_threads)
    }
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            request_timeout_secs: 30,
            worker_threads: 0,
            body_limit_mb: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub sqlx_logging: bool,
}

impl DatabaseSettings {
    pub fn acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.acquire_timeout_secs)
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            url: "postgres://app:app@localhost:55432/app".to_owned(),
            max_connections: 8,
            min_connections: 1,
            acquire_timeout_secs: 5,
            connect_timeout_secs: 5,
            sqlx_logging: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RedisSettings {
    pub url: String,
    pub ttl_secs: u64,
    pub pool_size: usize,
}

impl RedisSettings {
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_secs)
    }
}

impl Default for RedisSettings {
    fn default() -> Self {
        Self {
            url: "redis://localhost:56379".to_owned(),
            ttl_secs: 300,
            pool_size: 8,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct JwtSettings {
    pub issuer: String,
    pub audience: String,
    pub access_ttl_min: u64,
    pub refresh_ttl_days: u64,
    pub secret: String,
}

impl JwtSettings {
    pub fn access_ttl(&self) -> Duration {
        Duration::from_secs(self.access_ttl_min.saturating_mul(60))
    }

    pub fn refresh_ttl(&self) -> Duration {
        Duration::from_secs(self.refresh_ttl_days.saturating_mul(24 * 60 * 60))
    }
}

impl Default for JwtSettings {
    fn default() -> Self {
        Self {
            issuer: "template-api".to_owned(),
            audience: "template-front".to_owned(),
            access_ttl_min: 15,
            refresh_ttl_days: 7,
            secret: String::new(),
        }
    }
}

// O segredo nunca deve aparecer em log.
impl fmt::Debug for JwtSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtSettings")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("access_ttl_min", &self.access_ttl_min)
            .field("refresh_ttl_days", &self.refresh_ttl_days)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobSettings {
    pub enabled: bool,
    pub timezone: String,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            timezone: "America/Sao_Paulo".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogSettings {
    pub level: String,
    pub format: String,
    pub ansi: bool,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: "json".to_owned(),
            ansi: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsSettings {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub max_age_secs: u64,
}

impl Default for CorsSettings {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allowed_methods: ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
                .map(str::to_owned)
                .to_vec(),
            allowed_headers: [
                "authorization",
                "content-type",
                "idempotency-key",
                "x-request-id",
            ]
            .map(str::to_owned)
            .to_vec(),
            max_age_secs: 3600,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Settings {
        let mut settings = Settings::default();
        settings.jwt.secret = "x".repeat(MIN_SECRET_LEN);
        settings
    }

    #[test]
    fn aceita_configuracao_valida() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn rejeita_segredo_curto() {
        let mut settings = valid();
        settings.jwt.secret = "curto".to_owned();
        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("jwt.secret"), "{error}");
    }

    #[test]
    fn rejeita_timezone_invalido() {
        let mut settings = valid();
        settings.jobs.timezone = "Marte/Olympus".to_owned();
        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("jobs.timezone"), "{error}");
    }

    #[test]
    fn rejeita_pool_invertido() {
        let mut settings = valid();
        settings.database.min_connections = 20;
        settings.database.max_connections = 8;
        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("min_connections"), "{error}");
    }

    #[test]
    fn segredo_nao_aparece_no_debug() {
        let mut settings = valid();
        settings.jwt.secret = "segredo-que-nao-pode-vazar-1234567890".to_owned();
        let debug = format!("{settings:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("segredo-que-nao-pode-vazar"));
    }
}
