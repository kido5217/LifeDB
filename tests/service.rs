//! Task 3.2/3.3 verification: middleware stack behavior end to end.
//! Fixture routes (`/echo`, `/slow`) ride the production stack via
//! `build_with`, so tests exercise the same layers the binary serves.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use http_body_util::BodyExt;
use lifedb::app::build_with;
use lifedb::config::{Config, Environment, LogFormat};
use lifedb::state::AppState;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn test_config(body_limit_bytes: usize, request_timeout: Duration) -> Arc<Config> {
    Arc::new(Config {
        database_url: common::test_database_url(),
        listen_addr: "127.0.0.1:0".into(),
        environment: Environment::Dev,
        log_format: LogFormat::Pretty,
        rust_log: "info".into(),
        allowed_origins: vec!["http://localhost:5173".into()],
        cookie_secure: false,
        body_limit_bytes,
        request_timeout,
        session_ttl: Duration::from_secs(60),
    })
}

async fn echo(lifedb::error::Json(v): lifedb::error::Json<Value>) -> impl IntoResponse {
    axum::Json(v)
}

async fn slow() -> StatusCode {
    tokio::time::sleep(Duration::from_millis(500)).await;
    StatusCode::NO_CONTENT
}

fn rid() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

fn app() -> Router {
    // The fixture routes never touch the pool, so a lazy pool suffices.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&common::test_database_url())
        .expect("lazy test pool");
    let state = AppState::new(pool, test_config(1024, Duration::from_secs(30)));
    build_with(state, |api| api.route("/echo", post(echo)).route("/slow", post(slow)))
}

async fn response_json_and_status(res: axum::response::Response) -> (StatusCode, Value) {
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn success_response_carries_generated_request_id() {
    let res = app()
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rid = res
        .headers()
        .get(rid())
        .expect("request id header")
        .to_str()
        .unwrap()
        .to_string();
    let _: uuid::Uuid = rid.parse().expect("request id is a uuid");
}

#[tokio::test]
async fn client_request_id_is_respected_and_echoed() {
    let req = Request::builder()
        .uri("/api/v1/missing")
        .header(rid(), "client-chosen-1")
        .body(Body::empty())
        .unwrap();
    let res = app().oneshot(req).await.unwrap();
    let (status, body) = response_json_and_status(res).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["message"], "resource not found");
    assert_eq!(body["request_id"], "client-chosen-1");
}

#[tokio::test]
async fn envelope_request_id_matches_response_header() {
    let res = app()
        .oneshot(Request::builder().uri("/api/v1/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let header_rid = res
        .headers()
        .get(rid())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let (_, body) = response_json_and_status(res).await;
    assert_eq!(body["request_id"], header_rid);
}

#[tokio::test]
async fn oversized_body_is_413_envelope() {
    let big = vec![b'x'; 4096]; // over the 1024-byte test limit
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/echo")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(big))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json_and_status(res).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["message"], "payload too large");
    assert!(body["request_id"].is_string());
}

#[tokio::test]
async fn slow_handler_times_out_with_envelope() {
    let state = AppState::new(
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy(&common::test_database_url())
            .expect("lazy"),
        test_config(1024 * 1024, Duration::from_millis(50)),
    );
    let api_app = build_with(state, |api| api.route("/slow", post(slow)));
    let res = api_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/slow")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json_and_status(res).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(body["message"], "request timed out");
    assert!(body["request_id"].is_string());
}

#[tokio::test]
async fn foreign_origin_preflight_gets_no_cors_allow() {
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/echo")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(Body::empty())
        .unwrap();
    let res = app().oneshot(req).await.unwrap();
    assert!(
        !res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_some_and(|v| v == &HeaderValue::from_static("https://evil.example")),
        "foreign origin must not be reflected"
    );
}

#[tokio::test]
async fn configured_origin_preflight_is_allowed() {
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/echo")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(Body::empty())
        .unwrap();
    let res = app().oneshot(req).await.unwrap();
    assert_eq!(
        res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .map(|v| v.to_str().unwrap().to_string())
            .as_deref(),
        Some("http://localhost:5173")
    );
}

#[tokio::test]
async fn readiness_503_when_database_endpoint_is_dead() {
    let dead = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://postgres@localhost:5999/lifedb_nowhere")
        .expect("lazy pool");
    let state = AppState::new(dead, test_config(1024 * 1024, Duration::from_secs(30)));
    let api_app = build_with(state.clone(), |api| api);
    let res = api_app
        .oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, body) = response_json_and_status(res).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["message"], "service unavailable");
    assert!(body["request_id"].is_string());

    // Liveness keeps answering while readiness is down (spec scenario).
    let res = build_with(state, |api| api)
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
