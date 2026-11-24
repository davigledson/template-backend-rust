//! Regras de login, refresh, logout e perfil.
//!
//! - `login`: credenciais + rate limit + emissão do par;
//! - `session`: refresh com rotação/família e logout;
//! - `tokens`: emissão do access token e gravação do refresh.

mod login;
mod session;
mod tokens;

pub use login::login;
pub use session::{logout, refresh};
