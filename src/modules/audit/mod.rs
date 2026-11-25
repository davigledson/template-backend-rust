//! Auditoria de ações privilegiadas.
//!
//! O módulo é dono da tabela `audit_log`, append-only. Outros módulos gravam
//! através de `service::record_user`, na mesma transação da mudança.

pub mod controller;
pub mod dto;
mod repository;
pub mod service;

pub use controller::router;
