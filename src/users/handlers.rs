//! Auth handlers: register / login / logout / me (spec `user-auth`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use tower_sessions::Session;
use uuid::Uuid;
use validator::Validate;

use crate::auth;
use crate::error::{AppError, Json};
use crate::state::AppState;
use crate::users::{dto, queries};

const USER_ID_KEY: &str = "user_id";

/// The `/auth/*` sub-router, nested under `/api/v1` (design D12).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

/// Create an account; does not sign the client in (no session written).
async fn register(State(state): State<AppState>, Json(body): Json<dto::Credentials>) -> Result<StatusCode, AppError> {
    body.validate().map_err(validation_error)?;
    // Argon2id is deliberately CPU/memory heavy: keep it off the executor.
    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || auth::hash(&password))
        .await
        .map_err(internal)?
        .map_err(AppError::Internal)?;
    queries::insert(&state.pool, Uuid::now_v7(), &body.username, &hash).await?;
    Ok(StatusCode::CREATED)
}

/// Verify credentials, then create + persist a session. Unknown users get
/// the same work and the same 401 as wrong passwords.
async fn login(State(state): State<AppState>, session: Session, Json(body): Json<dto::Credentials>) -> Result<Response, AppError> {
    body.validate().map_err(validation_error)?;
    let user = queries::find_by_username(&state.pool, &body.username).await?;
    let password = body.password.clone();
    let ok = match user {
        Some(user) => {
            // Same blocking-pool path for known and unknown users, so the
            // enumeration defense holds on the timing channel too.
            let verified = tokio::task::spawn_blocking(move || auth::verify(&password, &user.password_hash))
                .await
                .map_err(internal)?;
            verified.then_some(user.id)
        }
        None => {
            tokio::task::spawn_blocking(move || auth::verify_against_dummy(&password))
                .await
                .map_err(internal)?;
            None
        }
    };
    let Some(id) = ok else {
        return Err(AppError::Unauthorized);
    };
    session.insert(USER_ID_KEY, id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Destroy the session and expire the cookie.
async fn logout(session: Session) -> Result<StatusCode, AppError> {
    session.flush().await.map_err(internal)?; // clears the record + expires the cookie
    Ok(StatusCode::NO_CONTENT)
}

/// Identity comes from the session record only (never from client fields).
async fn me(State(state): State<AppState>, session: Session) -> Result<impl IntoResponse, AppError> {
    let id: Uuid = session
        .get::<Uuid>(USER_ID_KEY)
        .await
        .map_err(internal)?
        .ok_or(AppError::Unauthorized)?;
    let row = sqlx::query_as!(dto::MeResponse, "SELECT id, username FROM users WHERE id = $1", id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(axum::Json(row))
}

fn validation_error(err: validator::ValidationErrors) -> AppError {
    let details = err
        .field_errors()
        .into_iter()
        .flat_map(|(_field, errors)| {
            errors
                .iter()
                .map(|e| {
                    e.message
                        .clone()
                        .unwrap_or_else(|| std::borrow::Cow::Borrowed("invalid value"))
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    AppError::Validation(details)
}

fn internal(err: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("{err}"))
}
