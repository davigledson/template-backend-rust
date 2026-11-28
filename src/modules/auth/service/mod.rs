//! Regras de login, refresh, logout e perfil.
//!
//! - `login`: credenciais + rate limit + emissão do par;
//! - `session`: refresh com rotação/família e logout;
//! - `tokens`: emissão do access token e gravação do refresh;
//! - `password`: recuperação de senha por código de e-mail;
//! - `verification`: confirmação de e-mail;
//! - `devices`: sessões por dispositivo.

mod devices;
mod login;
mod password;
mod session;
mod tokens;
mod verification;

pub use devices::{clear_sessions, list as list_sessions, revoke as revoke_session};
pub use login::login;
pub use password::{confirm_reset, request_reset};
pub use session::{logout, refresh};
pub use tokens::IssuedTokens;
pub use verification::{resend as resend_verification, verify as verify_email};
