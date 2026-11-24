//! Template de backend Rust — Axum + SeaORM + PostgreSQL + Redis.
//!
//! Arquitetura: monólito modular por feature.
//! `modules/<feature>` é dono das suas rotas, regras e tabelas;
//! `core` guarda apenas infraestrutura compartilhada.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![forbid(unsafe_code)]

pub mod app;
pub mod config;
pub mod core;
pub mod database;
pub mod routes;

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
