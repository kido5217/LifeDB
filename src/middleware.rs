//! The middleware stack (design D7, outermost -> innermost):
//!
//! ```text
//! set-request-id -> propagate-id -> envelope -> trace -> timeout
//!   -> cors -> compression -> body-limit -> session -> origin-check -> routes
//! ```
//!
//! The timeout and origin-guard layers are hand-rolled so their failures
//! produce the same JSON envelope as handler errors; everything else comes
//! from tower-http / axum. The `envelope` middleware is also the single
//! place that emits the one structured log event per completed request.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::header::{self, COOKIE, ORIGIN};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::trace::MakeSpan;
use tracing::{Span, error, info, info_span, warn};
use uuid::Uuid;

use crate::error::{AppError, EnvelopeMeta};
use crate::state::AppState;

/// Name of the session cookie, shared by the session layer and the origin guard.
pub const SESSION_COOKIE: &str = "lifedb_session";

/// The header carrying the correlation id (no `http`-crate constant exists for it).
pub fn rid_header() -> axum::http::HeaderName {
    axum::http::HeaderName::from_static("x-request-id")
}

/// Keep a client-sent `X-Request-Id`; otherwise assign a fresh uuid v7.
#[derive(Clone, Debug)]
pub struct RidMaker;

impl MakeRequestId for RidMaker {
    fn make_request_id<B>(&mut self, request: &axum::http::Request<B>) -> Option<RequestId> {
        if request.headers().contains_key(rid_header()) {
            return None;
        }
        Some(RequestId::new(
            HeaderValue::from_str(&Uuid::now_v7().to_string())
                .expect("uuid is a valid header value"),
        ))
    }
}

/// One span per request carrying rid, method and path.
#[derive(Clone, Debug)]
pub struct RequestSpan;

impl<S> MakeSpan<S> for RequestSpan {
    fn make_span(&mut self, request: &axum::http::Request<S>) -> Span {
        let rid = request
            .headers()
            .get(rid_header())
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string();
        info_span!(
            "request",
            rid = %rid,
            method = %request.method(),
            path = %request.uri().path(),
        )
    }
}

/// CORS with the configured origins pinned (never `Any`).
pub fn cors(origins: &HashSet<String>) -> CorsLayer {
    let list: Vec<HeaderValue> = origins
        .iter()
        .map(|o| {
            o.parse()
                .expect("configured origins are valid header values")
        })
        .collect();
    CorsLayer::new()
        .allow_origin(list)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600))
}

/// Timeout middleware returning the standard envelope on expiry.
pub async fn timeout(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let dur = state.config.request_timeout;
    match tokio::time::timeout(dur, next.run(req)).await {
        Ok(res) => res,
        Err(_) => AppError::Timeout.into_response(),
    }
}

/// Finish error responses with the request id, log every completed request,
/// and guarantee 5xx causes stay server-side (spec `api-service`).
pub async fn envelope(req: Request, next: Next) -> Response {
    let rid = req
        .headers()
        .get(rid_header())
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let res = next.run(req).await;
    let (mut parts, body) = res.into_parts();
    let meta = parts.extensions.remove::<EnvelopeMeta>();
    let status = parts.status;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if status.as_u16() >= 400 {
        let mut value = meta
            .as_ref()
            .map(|m| m.body.clone())
            .unwrap_or_else(|| json!({ "message": status.canonical_reason().unwrap_or("error") }));
        value["request_id"] = Value::String(rid.clone());
        parts.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        match meta.as_ref().and_then(|m| m.cause.clone()) {
            Some(cause) => {
                error!(target: "http", rid = %rid, method = %method, path = %path, status = status.as_u16(), %cause, elapsed_ms, "request failed")
            }
            None => {
                warn!(target: "http", rid = %rid, method = %method, path = %path, status = status.as_u16(), elapsed_ms, "request failed")
            }
        }
        Response::from_parts(parts, axum::body::Body::from(value.to_string()))
    } else {
        info!(target: "http", rid = %rid, method = %method, path = %path, status = status.as_u16(), elapsed_ms, "request");
        Response::from_parts(parts, body)
    }
}

/// Session-changing or origin-bearing mutating requests must match the
/// configured allow-list (spec `user-auth`, origin check requirement).
pub async fn origin_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if mutating {
        let origin = req
            .headers()
            .get(ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let has_session_cookie = req
            .headers()
            .get(COOKIE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|raw| {
                raw.split(';')
                    .any(|c| c.trim().starts_with(&format!("{SESSION_COOKIE}=")))
            });
        let allowed = origin.as_deref().is_some_and(|o| state.origins.contains(o));
        if (has_session_cookie || origin.is_some()) && !allowed {
            return Err(AppError::ForbiddenOrigin);
        }
    }
    Ok(next.run(req).await)
}

/// Build the request-id layers (outermost pair).
pub fn request_id_layers() -> (SetRequestIdLayer<RidMaker>, PropagateRequestIdLayer) {
    (
        SetRequestIdLayer::x_request_id(RidMaker),
        PropagateRequestIdLayer::x_request_id(),
    )
}
