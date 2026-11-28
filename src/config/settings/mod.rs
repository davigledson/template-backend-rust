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

mod audit;
mod cors;
mod database;
mod docs;
mod jobs;
mod jwt;
mod log;
mod mail;
mod metrics;
mod rate_limit;
mod redis;
mod security;
mod server;

use std::path::Path;

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use thiserror::Error;

pub use audit::AuditSettings;
pub use cors::CorsSettings;
pub use database::DatabaseSettings;
pub use docs::DocsSettings;
pub use jobs::JobSettings;
pub use jwt::JwtSettings;
pub use log::LogSettings;
pub use mail::MailSettings;
pub use metrics::MetricsSettings;
pub use rate_limit::RateLimitSettings;
pub use redis::RedisSettings;
pub use security::SecuritySettings;
pub use server::ServerSettings;

const DEFAULT_PROFILE: &str = "development";

fn valid_profile_name(profile: &str) -> bool {
    !profile.is_empty()
        && profile
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

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
    pub security: SecuritySettings,
    pub docs: DocsSettings,
    pub metrics: MetricsSettings,
    pub audit: AuditSettings,
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

        if !valid_profile_name(&profile) {
            return Err(SettingsError::Invalid(format!(
                "APP_PROFILE inválido: {profile}; use letras, números, '-' ou '_'"
            )));
        }

        let profile_file = format!("config/{profile}.toml");
        if profile != DEFAULT_PROFILE && !Path::new(&profile_file).exists() {
            return Err(SettingsError::Invalid(format!(
                "perfil desconhecido: {profile}; crie {profile_file} ou use {DEFAULT_PROFILE}"
            )));
        }

        if profile == DEFAULT_PROFILE {
            let _ = dotenvy::dotenv();
        }

        let mut figment = Figment::new().merge(Toml::file("default.toml"));

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
        self.metrics.collect_errors(&mut problems);
        self.cors.collect_errors(&mut problems);

        // Defaults de desenvolvimento não podem vazar para produção sem
        // revisão: e-mail em `log` descarta recuperação/confirmação de senha e
        // `/metrics` sem token expõe a telemetria.
        if self.profile == "production" {
            if self.mail.provider == "log" {
                problems.push(
                    "mail.provider = log não é permitido em produção; configure smtp".to_owned(),
                );
            }
            if self.metrics.enabled && self.metrics.token().is_none() {
                problems.push(
                    "metrics.token é obrigatório em produção quando metrics.enabled = true"
                        .to_owned(),
                );
            }
        }

        // Sem SMTP o código de confirmação só existe no outbox: ligar a
        // exigência de e-mail confirmado tornaria o login impossível.
        if self.security.require_email_verified && self.mail.provider != "smtp" {
            problems.push("security.require_email_verified exige mail.provider = smtp".to_owned());
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
            mail: MailSettings::default(),
            jobs: JobSettings::default(),
            rate_limit: RateLimitSettings::default(),
            log: LogSettings::default(),
            cors: CorsSettings::default(),
            security: SecuritySettings::default(),
            docs: DocsSettings::default(),
            metrics: MetricsSettings::default(),
            audit: AuditSettings::default(),
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

    #[test]
    fn perfil_aceita_apenas_nome_seguro() {
        assert!(valid_profile_name("development"));
        assert!(valid_profile_name("production"));
        assert!(valid_profile_name("staging-1"));
        assert!(!valid_profile_name(""));
        assert!(!valid_profile_name("../production"));
        assert!(!valid_profile_name("producao/../etc"));
    }

    #[test]
    fn confirmacao_obrigatoria_exige_smtp() {
        let mut settings = valid();
        settings.security.require_email_verified = true;
        settings.mail.provider = "log".to_owned();

        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("require_email_verified"), "{error}");

        settings.mail.provider = "smtp".to_owned();
        settings.mail.smtp_url = "smtp://localhost:1025".to_owned();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn producao_recusa_log_e_metrica_sem_token() {
        let mut settings = valid();
        settings.profile = "production".to_owned();

        let error = settings.validate().unwrap_err().to_string();
        assert!(error.contains("mail.provider"), "{error}");
        assert!(error.contains("metrics.token"), "{error}");
    }

    #[test]
    fn producao_aceita_smtp_e_metrica_com_token() {
        let mut settings = valid();
        settings.profile = "production".to_owned();
        settings.mail.provider = "smtp".to_owned();
        settings.mail.smtp_url = "smtp://localhost:1025".to_owned();
        settings.metrics.token = "token-de-teste-123456".to_owned();

        assert!(settings.validate().is_ok());
    }
}
