//! Group 4 verification: the auth vertical against the full production
//! stack (`build`) on the shared `lifedb_test` database.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{self, HeaderName};
use axum::http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use lifedb::app::build;
use lifedb::config::{Config, Environment, LogFormat};
use lifedb::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

fn rid() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

fn cfg(environment: Environment, session_ttl: Duration, cookie_secure: bool) -> Arc<Config> {
    Arc::new(Config {
        database_url: common::test_database_url(),
        listen_addr: "127.0.0.1:0".into(),
        environment,
        log_format: LogFormat::Pretty,
        rust_log: "info".into(),
        allowed_origins: vec!["http://localhost:5173".into()],
        cookie_secure,
        body_limit_bytes: 1024 * 1024,
        request_timeout: Duration::from_secs(30),
        session_ttl,
    })
}

async fn app() -> Router {
    let pool = common::test_pool().await;
    build(AppState::new(
        pool,
        cfg(Environment::Dev, Duration::from_secs(60), false),
    ))
}

fn post_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "http://localhost:5173")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn with_cookie(req: Request<Body>, cookie: &str) -> Request<Body> {
    let (mut parts, body) = req.into_parts();
    parts
        .headers
        .insert(header::COOKIE, cookie.parse().unwrap());
    Request::from_parts(parts, body)
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Option<Value>,
    raw: String,
}

async fn into_reply(res: axum::response::Response) -> Reply {
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let raw = String::from_utf8_lossy(&bytes).to_string();
    let body = serde_json::from_str(&raw).ok();
    Reply {
        status,
        headers,
        body,
        raw,
    }
}

/// Extract the opaque session value from a Set-Cookie header.
fn cookie_from(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string()
}

async fn register_with(app: Router, username: &str, password: &str, email: &str) -> Reply {
    into_reply(
        app.oneshot(post_json(
            "/api/v1/auth/register",
            &json!({"username": username, "password": password, "email": email}),
        ))
        .await
        .unwrap(),
    )
    .await
}

async fn register(app: Router, username: &str, password: &str) -> Reply {
    register_with(app, username, password, &format!("{username}@example.com")).await
}

async fn login(app: Router, username: &str, password: &str) -> Reply {
    into_reply(
        app.oneshot(post_json(
            "/api/v1/auth/login",
            &json!({"username": username, "password": password}),
        ))
        .await
        .unwrap(),
    )
    .await
}

#[tokio::test]
async fn register_creates_account_without_a_session() {
    let user = common::unique_username("reg");
    let reply = register(app().await, &user, "twelve chars pass!").await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert!(
        reply.headers.get(header::SET_COOKIE).is_none(),
        "register must not set a session cookie"
    );

    let dupe = register(app().await, &user, "twelve chars pass!").await;
    assert_eq!(dupe.status, StatusCode::CONFLICT);
    assert_eq!(dupe.body.unwrap()["code"], "username_taken");
}

#[tokio::test]
async fn register_rejects_weak_input_with_rule_details() {
    let reply = register(app().await, "alice", "short").await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let details = reply.body.unwrap()["details"].as_array().unwrap().clone();
    assert!(!details.is_empty(), "failed rules must be listed");
}

#[tokio::test]
async fn login_flow_sets_hardened_cookie_and_serves_me() {
    let router = app().await;
    let user = common::unique_username("flow");
    register(router.clone(), &user, "twelve chars pass!").await;

    let reply = login(router.clone(), &user, "twelve chars pass!").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let cookie = set_cookie_of(&reply);
    assert!(cookie.contains("lifedb_session="), "{cookie}");
    assert!(cookie.to_ascii_lowercase().contains("httponly"), "{cookie}");
    assert!(
        cookie.to_ascii_lowercase().contains("samesite=lax"),
        "{cookie}"
    );
    assert!(
        !cookie.to_ascii_lowercase().contains("secure"),
        "dev must not set Secure: {cookie}"
    );
    let cookie = cookie_from(&cookie);

    let me = into_reply(
        router
            .clone()
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body.as_ref().unwrap()["username"], user);

    // Tampered cookie value is rejected.
    let tampered = cookie.replace("lifedb_session=", "lifedb_session=x");
    let me = into_reply(
        router
            .clone()
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &tampered,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);

    // Logout expires the cookie; the same value no longer authenticates.
    let out = into_reply(
        router
            .clone()
            .oneshot(with_cookie(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/logout")
                    .header(header::ORIGIN, "http://localhost:5173")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(
        out.headers.contains_key(header::SET_COOKIE),
        "logout must expire the cookie"
    );
    let me = into_reply(
        router
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

fn set_cookie_of(reply: &Reply) -> String {
    reply
        .headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn prod_config_marks_the_cookie_secure() {
    let pool = common::test_pool().await;
    let router = build(AppState::new(
        pool,
        cfg(Environment::Prod, Duration::from_secs(60), true),
    ));
    let user = common::unique_username("sec");
    let reg = register(router.clone(), &user, "twelve chars pass!").await;
    assert_eq!(
        reg.status,
        StatusCode::CREATED,
        "prod register: {:?}",
        reg.body
    );
    let reply = login(router, &user, "twelve chars pass!").await;
    assert_eq!(
        reply.status,
        StatusCode::NO_CONTENT,
        "prod login: {:?}",
        reply.body
    );
    assert!(
        set_cookie_of(&reply)
            .to_ascii_lowercase()
            .contains("secure")
    );
}

#[tokio::test]
async fn expired_session_is_unauthorized() {
    let pool = common::test_pool().await;
    let router = build(AppState::new(
        pool,
        cfg(Environment::Dev, Duration::from_secs(1), false),
    ));
    let user = common::unique_username("ttl");
    register(router.clone(), &user, "twelve chars pass!").await;
    let reply = login(router.clone(), &user, "twelve chars pass!").await;
    let cookie = cookie_from(&set_cookie_of(&reply));
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let me = into_reply(
        router
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn restart_revokes_sessions() {
    let user = common::unique_username("restart");
    register(app().await, &user, "twelve chars pass!").await;
    let reply = login(app().await, &user, "twelve chars pass!").await;
    let cookie = cookie_from(&set_cookie_of(&reply));

    // A freshly built router is a "restarted" server: same store type, no memory.
    let me = into_reply(
        app()
            .await
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_user_and_wrong_password_are_indistinguishable() {
    let user = common::unique_username("timing");
    register(app().await, &user, "twelve chars pass!").await;

    let attempt = |app: Router, username: String, rid_value: &'static str| async move {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, "http://localhost:5173")
            .header(rid(), rid_value)
            .body(Body::from(
                json!({"username": username, "password": "wrong password here"}).to_string(),
            ))
            .unwrap();
        let start = std::time::Instant::now();
        let reply = into_reply(app.oneshot(req).await.unwrap()).await;
        (start.elapsed(), reply)
    };

    let (wrong_time, wrong) = attempt(app().await, user.clone(), "fixed-1").await;
    let (unknown_time, unknown) = attempt(app().await, "ghost_zzz".to_string(), "fixed-1").await;

    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong.raw, unknown.raw,
        "401 envelopes must be byte-identical"
    );

    // Loose smoke bound: within one order of magnitude of each other.
    let (slow, fast) = if wrong_time > unknown_time {
        (wrong_time, unknown_time)
    } else {
        (unknown_time, wrong_time)
    };
    assert!(
        fast > Duration::from_millis(5),
        "measurement below hashing floor: wrong={wrong_time:?} unknown={unknown_time:?}"
    );
    assert!(
        slow < fast.mul_f32(10.0),
        "timing split: wrong={wrong_time:?} unknown={unknown_time:?}"
    );
}

#[tokio::test]
async fn foreign_origin_is_rejected_on_mutating_requests() {
    let router = app().await;
    let user = common::unique_username("origin");
    register(router.clone(), &user, "twelve chars pass!").await;
    let reply = login(router.clone(), &user, "twelve chars pass!").await;
    let cookie = cookie_from(&set_cookie_of(&reply));

    // Session-bearing POST from a foreign origin -> 403.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let out = into_reply(
        router
            .clone()
            .oneshot(with_cookie(req, &cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(out.status, StatusCode::FORBIDDEN);
    assert_eq!(out.body.as_ref().unwrap()["message"], "origin not allowed");

    // Login (no cookie yet) from a foreign origin -> 403.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from(
            json!({"username": user, "password": "twelve chars pass!"}).to_string(),
        ))
        .unwrap();
    let reply = into_reply(router.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);

    // Same-origin GET passes (session is still alive after the 403s above).
    let me = into_reply(
        router
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        me.status,
        StatusCode::OK,
        "me body: {:?} raw: {}",
        me.body,
        me.raw
    );
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let user = common::unique_username("badpw");
    register(app().await, &user, "twelve chars pass!").await;
    let reply = login(app().await, &user, "a different password").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn duplicate_email_409_in_both_arrival_orders() {
    // Pair 1: lowercase arrives first, uppercase-only variant conflicts.
    let u1 = common::unique_username("dupa");
    let u2 = common::unique_username("dupb");
    let e1 = format!("{u1}@dup.example");
    let e1_upper = format!("{u1}@DUP.EXAMPLE");
    assert_eq!(
        register_with(app().await, &u1, "twelve chars pass!", &e1)
            .await
            .status,
        StatusCode::CREATED
    );
    let dupe = register_with(app().await, &u2, "twelve chars pass!", &e1_upper).await;
    assert_eq!(dupe.status, StatusCode::CONFLICT);
    assert_eq!(dupe.body.unwrap()["code"], "unique_violation");

    // Pair 2: reversed order, distinct address.
    let u3 = common::unique_username("dupc");
    let u4 = common::unique_username("dupd");
    let e2 = format!("{u3}@dup.example");
    let e2_upper = format!("{u3}@DUP.EXAMPLE");
    assert_eq!(
        register_with(app().await, &u3, "twelve chars pass!", &e2_upper)
            .await
            .status,
        StatusCode::CREATED
    );
    let dupe = register_with(app().await, &u4, "twelve chars pass!", &e2).await;
    assert_eq!(dupe.status, StatusCode::CONFLICT);

    let pool = common::test_pool().await;
    for email in [&e1, &e2] {
        let rows: i64 = sqlx::query_scalar!(
            "SELECT count(*) FROM users WHERE lower(email) = lower($1)",
            email
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);
        assert_eq!(
            rows, 1,
            "case-folded uniqueness must keep exactly one row per address"
        );
    }
}

#[tokio::test]
async fn register_rejects_malformed_email_with_rule_detail() {
    let user = common::unique_username("bademail");
    let reply = register_with(app().await, &user, "twelve chars pass!", "not-an-email").await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let details = reply.body.unwrap()["details"].as_array().unwrap().clone();
    assert!(
        details
            .iter()
            .any(|d| d.as_str().is_some_and(|s| s.contains("valid email"))),
        "{details:?}"
    );
}

#[tokio::test]
async fn login_with_email_gets_validation_400() {
    let user = common::unique_username("emailogin");
    let email = format!("{user}@example.com");
    register_with(app().await, &user, "twelve chars pass!", &email).await;
    // The email in the username field fails the username rule before any
    // lookup: never a 401/200, no email column consulted.
    let reply = login(app().await, &email, "twelve chars pass!").await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.body.unwrap()["message"], "request validation failed");
}

#[tokio::test]
async fn me_exposes_email_and_legacy_rows_read_null() {
    let user = common::unique_username("meemail");
    // Mixed-case, run-unique address: proves original-case storage.
    let email = format!("{}a@Example.com", user);
    register_with(app().await, &user, "twelve chars pass!", &email).await;
    let router = app().await;
    let reply = login(router.clone(), &user, "twelve chars pass!").await;
    let cookie = cookie_from(&set_cookie_of(&reply));
    let me = into_reply(
        router
            .clone()
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body.as_ref().unwrap()["email"], email);

    // A row inserted without email (pre-0002 legacy shape) authenticates and
    // reads email: null.
    let legacy = common::unique_username("legacy");
    let pool = common::test_pool().await;
    sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (uuidv7(), $1, $2)")
        .bind(&legacy)
        .bind(lifedb::auth::hash("twelve chars pass!").unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let reply = login(router.clone(), &legacy, "twelve chars pass!").await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let cookie = cookie_from(&set_cookie_of(&reply));
    let me = into_reply(
        router
            .oneshot(with_cookie(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .body(Body::empty())
                    .unwrap(),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me.status, StatusCode::OK);
    assert!(me.body.as_ref().unwrap()["email"].is_null());
}
