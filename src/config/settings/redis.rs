//! Conexão com o Redis (refresh token, rate limit e idempotência).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RedisSettings {
    pub url: String,
    pub pool_size: usize,
}

impl RedisSettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.url.trim().is_empty() {
            problems.push("redis.url não pode ser vazia".to_owned());
        }
        if self.pool_size == 0 {
            problems.push("redis.pool_size deve ser maior que zero".to_owned());
        }
    }
}

impl Default for RedisSettings {
    fn default() -> Self {
        Self {
            url: "redis://localhost:56379".to_owned(),
            pool_size: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &RedisSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_pool_zero() {
        assert!(errors(&RedisSettings::default()).is_empty());

        let settings = RedisSettings {
            pool_size: 0,
            ..Default::default()
        };
        assert!(errors(&settings).contains("redis.pool_size"));
    }
}
