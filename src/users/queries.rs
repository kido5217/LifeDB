//! Account persistence: rows, insert, and username lookup.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Insert a new account; duplicate usernames surface as `AppError::Database`
/// (mapped to 409 by the envelope rules).
pub async fn insert(
    pool: &PgPool,
    id: Uuid,
    username: &str,
    password_hash: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO users (id, username, password_hash) VALUES ($1, $2, $3)",
        id,
        username,
        password_hash
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Look up an account by its exact username.
pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as!(
        User,
        "SELECT id, username, password_hash, created_at FROM users WHERE username = $1",
        username
    )
    .fetch_optional(pool)
    .await?;
    Ok(user)
}
