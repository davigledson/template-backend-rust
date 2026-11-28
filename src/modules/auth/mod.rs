//! Endpoints de autenticação.

pub mod controller;
pub mod cookie;
pub mod dto;
mod repository;
pub mod service;

pub use controller::router;
