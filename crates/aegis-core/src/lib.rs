//! Aegis VPN core: domain types, connection lifecycle engine, non-secret
//! storage, diagnostics and the platform backend trait.
//!
//! This crate has no Tauri or platform-specific dependencies.

pub mod backend;
pub mod diagnostics;
pub mod draft;
pub mod engine;
pub mod environment;
pub mod error;
pub mod mock;
pub mod registry;
pub mod retry;
pub mod store;
pub mod types;

pub use backend::VpnBackend;
pub use engine::Engine;
pub use error::{CoreError, Result};
pub use types::*;

#[cfg(test)]
mod engine_tests;
