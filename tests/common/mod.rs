//! Shared integration-test setup: the `lifedb_test` database on the dev cluster.
//!
//! This module is compiled into every test binary; each uses a subset.
#![allow(dead_code)]

use sqlx::postgres::PgPool;
use tokio::sync::OnceCell;

/// URL of the test database, derived from the dev `DATABASE_URL` contract.
pub fn test_database_url() -> String {
    let base = std::env::var("DATABASE_URL").expect("dev shell exports DATABASE_URL");
    let (head, _) = base.rsplit_once('/').expect("URL with a database segment");
    format!("{head}/lifedb_test")
}

/// Ensure `lifedb_test` exists and is migrated; the pool is created once
/// per test process and shared across tests.
pub async fn test_pool() -> PgPool {
    static POOL: OnceCell<PgPool> = OnceCell::const_new();
    POOL.get_or_init(|| async {
        let url = test_database_url();
        let admin = lifedb::db::connect(
            &std::env::var("DATABASE_URL").expect("dev shell exports DATABASE_URL"),
        )
        .await
        .expect("dev cluster must be reachable for test setup");
        // Create the test database through the live `lifedb` connection when
        // absent; 42P04 (duplicate_database) means it already exists.
        if let Err(err) = sqlx::query("CREATE DATABASE lifedb_test")
            .execute(&admin)
            .await
        {
            let duplicate = matches!(
                err.as_database_error().map(|d| d.code().map(|c| c.to_string())),
                Some(Some(code)) if code == "42P04"
            );
            assert!(duplicate, "creating lifedb_test: {err}");
        }
        drop(admin);
        let pool = lifedb::db::connect(&url)
            .await
            .expect("pool against lifedb_test");
        lifedb::db::MIGRATIONS
            .run(&pool)
            .await
            .expect("migrations on lifedb_test");
        pool
    })
    .await
    .clone()
}

/// A username unique across processes and runs (pid + wall-clock suffix),
/// so parallel test binaries and repeated runs never collide; rows from
/// earlier runs are therefore harmless and need no truncation. Result is
/// lowercase `[a-z0-9_]` and <= 32 chars with the tags used here.
pub fn unique_username(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after 1970")
        .as_nanos();
    format!(
        "{tag}_{}_{:x}",
        std::process::id(),
        nanos & 0xffff_ffff_ffff
    )
}
