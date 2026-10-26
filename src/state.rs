//! Shared service state: the database pool and the parsed configuration.

use std::collections::HashSet;
use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    /// Parsed `ALLOWED_ORIGINS` set, shared by the CORS and origin layers.
    pub origins: Arc<HashSet<String>>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        let origins = config.allowed_origins.iter().cloned().collect();
        Self {
            pool,
            config,
            origins: Arc::new(origins),
        }
    }
}
