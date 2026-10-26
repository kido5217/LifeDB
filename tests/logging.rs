//! Spec `api-service` / structured request logging: exactly one log event
//! per completed request, carrying method, path, status, duration, rid.

mod common;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::Request;
use http_body_util::BodyExt;
use lifedb::app::build;
use lifedb::config::{Config, Environment, LogFormat};
use lifedb::state::AppState;
use serde_json::Value;
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Collect(Arc<Mutex<Vec<u8>>>);

impl MakeWriter<'_> for Collect {
    type Writer = Self;
    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

impl std::io::Write for Collect {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn exactly_one_event_per_request() {
    let collect = Collect::default();
    let subscriber = lifedb::telemetry::subscriber_for(collect.clone(), true, "info");
    tracing::subscriber::set_global_default(subscriber).expect("first subscriber in this binary");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&common::test_database_url())
        .unwrap();
    let router = build(AppState::new(
        pool,
        Arc::new(Config {
            database_url: common::test_database_url(),
            listen_addr: "127.0.0.1:0".into(),
            environment: Environment::Dev,
            log_format: LogFormat::Json,
            rust_log: "info".into(),
            allowed_origins: vec!["http://localhost:5173".into()],
            cookie_secure: false,
            body_limit_bytes: 1024 * 1024,
            request_timeout: std::time::Duration::from_secs(30),
            session_ttl: std::time::Duration::from_secs(60),
        }),
    ));
    let res = router
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let _ = res.into_body().collect().await;

    let output = String::from_utf8(collect.0.lock().unwrap().clone()).unwrap();
    let events: Vec<Value> = output
        .lines()
        .filter_map(|l| l.parse().ok())
        .filter(|v: &Value| v["fields"]["message"] == "request")
        .collect();
    assert_eq!(
        events.len(),
        1,
        "expected one request event, got:\n{output}"
    );
    let e = &events[0];
    assert_eq!(e["fields"]["method"], "GET");
    assert_eq!(e["fields"]["path"], "/healthz");
    assert_eq!(e["fields"]["status"], 200);
    assert!(e["fields"]["elapsed_ms"].is_number(), "duration field: {e}");
    assert!(
        e["fields"]["rid"].as_str().is_some_and(|s| !s.is_empty()),
        "rid field: {e}"
    );
}
