//! LifeDB REST API service.
//!
//! Module layout and every structural decision are recorded in
//! `openspec/changes/define-rust-api-stack/design.md` (D1-D12).
//! Modules arrive with their implementing tasks.

pub mod config;
pub mod db;
pub mod telemetry;

pub use config::{Config, ConfigError};
