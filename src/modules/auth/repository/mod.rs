//! Refresh tokens opacos no Redis.
//!
//! - token aleatório (não JWT), guardado apenas como hash SHA-256;
//! - valor `user_id:token_version:family_id` permite invalidar todas as sessões
//!   quando a senha ou o papel muda (`token_version`) e revogar uma família
//!   inteira quando um token já consumido é reapresentado;
//! - a rotação é atômica e deixa um marcador de "já usado" com o mesmo TTL.
//!
//! Dividido entre o ciclo do token (`token`) e o marcador de família (`family`).

mod family;
mod password_reset;
mod sessions;
mod ticket;
mod token;

use std::time::Duration;

use uuid::Uuid;

pub use family::{is_family_revoked, revoke_family};
pub use password_reset::{consume as consume_reset_token, issue as issue_reset_token};
pub use sessions::{
    SessionRecord, clear as clear_sessions, list as list_sessions, register as register_session,
    remove as remove_session, revoke_owned as revoke_owned_session, touch as touch_session,
};
pub use token::{consume, generate_token, new_family_id, peek, revoke, store};

const FALLBACK_TTL_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct RefreshTicket {
    pub user_id: Uuid,
    pub token_version: i32,
    pub family_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum ConsumeOutcome {
    /// Token válido; pode ser trocado por um par novo da mesma família.
    Active(RefreshTicket),
    /// Token já consumido; a família inteira deve ser revogada.
    Reused(Uuid),
    /// Token inexistente, expirado ou malformado.
    Unknown,
}

pub(super) fn ttl_secs(ttl: Duration) -> i64 {
    i64::try_from(ttl.as_secs())
        .unwrap_or(FALLBACK_TTL_SECS)
        .max(1)
}
