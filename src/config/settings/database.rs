//! Pool de conexões com o Postgres.

use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub connect_timeout_secs: u64,
    /// Limite por statement no pool da API; `0` desliga.
    pub statement_timeout_secs: u64,
    pub sqlx_logging: bool,
}

impl DatabaseSettings {
    pub fn acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.acquire_timeout_secs)
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    pub fn statement_timeout(&self) -> Option<Duration> {
        (self.statement_timeout_secs > 0).then(|| Duration::from_secs(self.statement_timeout_secs))
    }

    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.url.trim().is_empty() {
            problems.push("database.url não pode ser vazia".to_owned());
        }
        if self.max_connections == 0 || self.max_connections > 100 {
            problems.push("database.max_connections deve estar entre 1 e 100".to_owned());
        }
        if self.min_connections > self.max_connections {
            problems.push("database.min_connections não pode exceder max_connections".to_owned());
        }
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
            statement_timeout_secs: 5,
            sqlx_logging: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &DatabaseSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_url_vazia() {
        let settings = DatabaseSettings {
            url: "   ".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("database.url"));
    }

    #[test]
    fn rejeita_pool_invertido() {
        let settings = DatabaseSettings {
            min_connections: 20,
            max_connections: 8,
            ..Default::default()
        };
        assert!(errors(&settings).contains("min_connections"));
    }

    #[test]
    fn rejeita_pool_acima_do_limite() {
        let settings = DatabaseSettings {
            max_connections: 101,
            ..Default::default()
        };
        assert!(errors(&settings).contains("entre 1 e 100"));
    }

    #[test]
    fn statement_timeout_zero_desliga() {
        let mut settings = DatabaseSettings::default();
        assert_eq!(settings.statement_timeout(), Some(Duration::from_secs(5)));

        settings.statement_timeout_secs = 0;
        assert!(settings.statement_timeout().is_none());
    }
}
