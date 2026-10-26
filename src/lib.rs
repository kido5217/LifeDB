//! LifeDB REST API service.
//!
//! Module layout and every structural decision are recorded in
//! `openspec/changes/define-rust-api-stack/design.md` (D1-D12).

pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod middleware;
pub mod state;
pub mod telemetry;

pub use config::{Config, ConfigError};
pub use error::AppError;
pub use state::AppState;
