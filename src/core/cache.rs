//! Cache Redis (best-effort).
//!
//! A API deve continuar de pé quando o Redis cair: as operações de cache
//! registram o erro e seguem sem cache. O ping é usado apenas no readiness.

use deadpool_redis::{Config, Pool, PoolConfig, Runtime};
use thiserror::Error;

use crate::config::settings::RedisSettings;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("não foi possível criar o pool do Redis: {0}")]
    CreatePool(#[from] deadpool_redis::CreatePoolError),
    #[error("pool do Redis indisponível: {0}")]
    Pool(#[from] deadpool_redis::PoolError),
    #[error("falha na conexão com o Redis: {0}")]
    Redis(#[from] redis::RedisError),
}

#[derive(Clone)]
pub struct Cache {
    pool: Pool,
}

impl Cache {
    pub fn connect(settings: &RedisSettings) -> Result<Self, CacheError> {
        let mut config = Config::from_url(settings.url.clone());
        config.pool = Some(PoolConfig {
            max_size: settings.pool_size,
            ..PoolConfig::default()
        });

        let pool = config.create_pool(Some(Runtime::Tokio1))?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Usado pelo `/health/ready`.
    pub async fn ping(&self) -> Result<(), CacheError> {
        let mut connection = self.pool.get().await?;
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await?;
        Ok(())
    }
}
