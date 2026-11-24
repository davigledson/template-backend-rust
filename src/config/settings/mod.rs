//! Configuração tipada, carregada em camadas.
//!
//! Precedência (do menor para o maior):
//! `default.toml` → `config/<perfil>.toml` → variáveis `APP__*` → `.env` (só dev).
//!
//! O `.env` não é uma camada do Figment: ele apenas popula variáveis de ambiente
//! quando o perfil é `development`.
//!
//! Cada seção tem seu próprio arquivo, com o mesmo nome da chave no TOML, e
//! valida as próprias invariantes em `collect_errors`. Este módulo só monta o
//! agregado.

mod cors;
mod database;
mod jobs;
mod jwt;
mod log;
mod mail;
mod rate_limit;
mod redis;
mod server;

use std::path::Path;

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use thiserror::Error;

pub use cors::CorsSettings;
pub use database::DatabaseSettings;
pub use jobs::JobSettings;
pub use jwt::JwtSettings;
pub use log::LogSettings;
pub use mail::MailSettings;
pub use rate_limit::RateLimitSettings;
pub use redis::RedisSettings;
pub use server::ServerSettings;

const DEFAULT_PROFILE: &str = "development";

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
    pub mail: MailSettings,
    pub jobs: JobSettings,
    pub rate_limit: RateLimitSettings,
    pub log: LogSettings,
    pub cors: CorsSettings,
}

impl Settings {
    /// Carrega `default.toml`, o perfil ativo e os overrides `APP__*`.
    pub fn load() -> Result<Self, SettingsError> {
        let settings = Self::load_unvalidated()?;
        settings.validate()?;
        Ok(settings)
    }

    /// Carrega sem validar invariantes.
    ///
    /// Usado por migrations e CLI que só precisam do banco. A API e o worker
    /// sempre usam [`Settings::load`].
    pub fn load_unvalidated() -> Result<Self, SettingsError> {
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
        Ok(settings)
    }

    /// Valida invariantes que não fazem sentido só no tipo.
    ///
    /// Cada seção contribui com as suas; o resultado acumula todas as falhas em
    /// vez de parar na primeira.
    pub fn validate(&self) -> Result<(), SettingsError> {
        let mut problems: Vec<String> = Vec::new();

        self.server.collect_errors(&mut problems);
        self.database.collect_errors(&mut problems);
        self.redis.collect_errors(&mut problems);
        self.rate_limit.collect_errors(&mut problems);
        self.jwt.collect_errors(&mut problems);
        self.mail.collect_errors(&mut problems);
        self.jobs.collect_errors(&mut problems);
        self.log.collect_errors(&mut problems);

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
            mail: MailSettings::default(),
            jobs: JobSettings::default(),
            rate_limit: RateLimitSettings::default(),
            log: LogSettings::default(),
            cors: CorsSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Settings {
        let mut settings = Settings::default();
        settings.jwt.secret = "x".repeat(jwt::MIN_SECRET_LEN);
        settings
    }

    #[test]
    fn aceita_configuracao_valida() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn coleta_todos_os_problemas_de_uma_vez() {
        let mut settings = valid();
        settings.jwt.secret = "curto".to_owned();
        settings.jobs.timezone = "Marte/Olympus".to_owned();

        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("jwt.secret"), "{error}");
        assert!(error.contains("jobs.timezone"), "{error}");
    }
}
