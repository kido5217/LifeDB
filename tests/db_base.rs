//! Task 2.3 verification: the pool helper and readiness check against
//! `lifedb_test`; liveness of the check against a dead endpoint.

mod common;

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn readiness_ok_against_test_database() {
    let pool = common::test_pool().await;
    assert!(lifedb::db::is_ready(&pool).await);
}

#[tokio::test]
async fn readiness_fails_against_dead_endpoint() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://postgres@localhost:5999/lifedb_nowhere");
    assert!(!lifedb::db::is_ready(&pool.expect("lazy pool")).await);
}
