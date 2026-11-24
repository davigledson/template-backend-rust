//! E-mail transacional com outbox no Postgres.
//!
//! É o dono da tabela `email_outbox`. Outros módulos chamam `service::enqueue`
//! dentro da própria transação; o worker entrega.

mod repository;
pub mod service;
pub mod templates;
