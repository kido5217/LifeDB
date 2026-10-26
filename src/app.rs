//! Router assembly and the health probes.
//!
//! `build` produces the production surface (design D12); `build_with` lets
//! integration tests add fixture routes under `/api/v1` while reusing the
//! exact middleware stack. Layers are applied innermost-first (axum's
//! `.layer` wraps what precedes it), yielding the D7 order.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

use crate::{db, error::AppError, middleware, state::AppState};

/// The full service router for `state`.
pub fn build(state: AppState) -> Router {
    build_with(state, |api| api)
}

/// Router assembly with a hook to seed the `/api/v1` sub-router.
pub fn build_with(state: AppState, api: impl FnOnce(Router<AppState>) -> Router<AppState>) -> Router {
    let cfg = Arc::clone(&state.config);
    let sessions = SessionManagerLayer::new(MemoryStore::default())
        .with_name(middleware::SESSION_COOKIE)
        .with_secure(cfg.cookie_secure)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::seconds(
            cfg.session_ttl.as_secs().min(i64::MAX as u64) as i64,
        )));

    let api = api(Router::new());

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .nest("/api/v1", api)
        .fallback(fallback)
        // innermost -> outermost
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::origin_guard,
        ))
        .layer(sessions)
        .layer(axum::extract::DefaultBodyLimit::max(cfg.body_limit_bytes))
        .layer(
            tower_http::compression::CompressionLayer::new()
                .gzip(true)
                .br(true),
        )
        .layer(middleware::cors(
            &cfg.allowed_origins.iter().cloned().collect::<std::collections::HashSet<_>>(),
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::timeout,
        ))
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(middleware::RequestSpan)
                .on_response(()), // the one request event is emitted by `envelope`
        )
        .layer(axum::middleware::from_fn(middleware::envelope))
        .layer(middleware::request_id_layers().1)
        .layer(middleware::request_id_layers().0)
        .with_state(state)
}

/// Liveness: dependency-free (spec `api-service`, health probes).
async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: succeeds only with a live database round-trip.
async fn readyz(State(state): State<AppState>) -> Response {
    if db::is_ready(&state.pool).await {
        StatusCode::OK.into_response()
    } else {
        AppError::Unavailable.into_response()
    }
}

/// Anything unmatched becomes the standard 404 envelope.
async fn fallback() -> AppError {
    AppError::NotFound
}
