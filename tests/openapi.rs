//! Task 5.1 verification: the generated OpenAPI document.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use lifedb::app::build;
use lifedb::config::{Config, Environment, LogFormat};
use lifedb::state::AppState;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn openapi_document_describes_the_whole_surface() {
    let pool = common::test_pool().await;
    let router = build(AppState::new(
        pool,
        Arc::new(Config {
            database_url: common::test_database_url(),
            listen_addr: "127.0.0.1:0".into(),
            environment: Environment::Dev,
            log_format: LogFormat::Pretty,
            rust_log: "info".into(),
            allowed_origins: vec!["http://localhost:5173".into()],
            cookie_secure: false,
            body_limit_bytes: 1024 * 1024,
            request_timeout: Duration::from_secs(30),
            session_ttl: Duration::from_secs(60),
        }),
    ));
    let res = router
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let doc: Value = serde_json::from_slice(&bytes).expect("valid JSON document");

    for path in [
        "/api/v1/auth/register",
        "/api/v1/auth/login",
        "/api/v1/auth/logout",
        "/api/v1/auth/me",
    ] {
        assert!(
            doc["paths"][path].is_object(),
            "document lacks {path}: {:?}",
            doc["paths"]
                .as_object()
                .map(|m| m.keys().collect::<Vec<_>>())
        );
    }
    for schema in [
        "Credentials",
        "RegisterCredentials",
        "MeResponse",
        "ErrorEnvelope",
    ] {
        assert!(
            doc["components"]["schemas"][schema].is_object(),
            "schema {schema} missing"
        );
    }
    // Login payload stays {username, password}; register and me gained email.
    assert!(doc["components"]["schemas"]["Credentials"]["properties"]["email"].is_null());
    assert!(
        doc["components"]["schemas"]["RegisterCredentials"]["properties"]["email"].is_object(),
        "register body must expose email"
    );
    assert!(
        doc["components"]["schemas"]["MeResponse"]["properties"]["email"].is_object(),
        "me response must expose email"
    );
    assert_eq!(
        doc["paths"]["/api/v1/auth/register"]["post"]["responses"]["409"]["description"],
        "username or email taken"
    );
    assert!(
        doc["components"]["securitySchemes"]["sessionCookie"].is_object(),
        "cookie security scheme missing"
    );
    // Error responses reference the envelope component.
    assert_eq!(
        doc["paths"]["/api/v1/auth/login"]["post"]["responses"]["401"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/ErrorEnvelope"
    );
}
