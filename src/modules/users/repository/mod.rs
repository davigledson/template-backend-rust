//! Queries da tabela `users`. Só este módulo escreve nela.
//!
//! Dividido entre leitura (busca, listagem, contagens) e escrita (insert e
//! updates com concorrência otimista). O `mod.rs` reexporta a API pública.

mod read;
mod write;

pub use read::{
    count_active_admins, count_by_email, find_by_email, find_by_id, list, list_unverified,
};
pub use write::{insert, set_email_verified, set_password, set_role, update};
