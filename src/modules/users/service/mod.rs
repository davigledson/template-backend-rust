//! Regras do módulo `users`.
//!
//! É a porta pública para os outros módulos (ex.: `auth` busca usuário aqui).
//! Dividido por sub-recurso, para nenhum arquivo acumular responsabilidades:
//!
//! - `lookup`: leituras usadas por outros módulos;
//! - `create`/`update`: gestão feita por administrador;
//! - `credentials`: redefinição de senha (admin), recuperação por código e
//!   troca da própria senha;
//! - `profile`: o próprio nome;
//! - `session`: validação do token contra o estado atual do banco;
//! - `rules`: validações puras compartilhadas.

mod create;
mod credentials;
mod lookup;
mod profile;
mod rules;
mod session;
mod update;
mod verification;

pub use create::{NewUser, create};
pub use credentials::{change_own_password, reset_password_with_code, set_password};
pub use lookup::{find_by_email, find_by_id, list, normalize_email};
pub use profile::update_profile;
pub use session::{count_active_admins, require_session};
pub use update::{UserUpdate, deactivate, set_role, update};
pub use verification::{
    mark_email_verified, send_email_verification, verify_all_emails, verify_email,
};
