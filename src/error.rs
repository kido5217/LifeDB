//! Typed error model and the single JSON error envelope (design D6).
//!
//! Handlers return `Result<_, AppError>`. `IntoResponse` records status and
//! body metadata on the response; the `envelope` middleware (see
//! `middleware.rs`) injects the request id, serializes the body, and logs
//! the cause for 5xx so internal detail never crosses the wire.

use axum::Json as AxumJson;
use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;

/// The single JSON error body shape served for every generated error
/// (documented in OpenAPI; the runtime body is built by the envelope
/// middleware).
#[derive(Debug, utoipa::ToSchema)]
pub struct ErrorEnvelope {
    /// Client-safe summary of the failure.
    pub message: String,
    /// Correlation id, identical to the `x-request-id` response header.
    pub request_id: String,
    /// Present on 400: one entry per failed validation rule.
    pub details: Option<Vec<String>>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("request validation failed")]
    Validation(Vec<String>),
    #[error("resource not found")]
    NotFound,
    #[error("conflict")]
    Conflict(&'static str),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden origin")]
    ForbiddenOrigin,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("request timed out")]
    Timeout,
    #[error("service unavailable")]
    Unavailable,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("database failure")]
    Database(#[from] sqlx::Error),
}

/// Response metadata produced by `IntoResponse`, finished by the
/// envelope middleware (which knows the request id).
#[derive(Debug, Clone)]
pub struct EnvelopeMeta {
    pub body: Value,
    pub cause: Option<String>,
}

impl AppError {
    /// Split into the status, envelope body (without `request_id`), and the
    /// cause to log for 5xx.
    pub(crate) fn parts(&self) -> (StatusCode, Value, Option<String>) {
        match self {
            Self::Validation(fields) => (
                StatusCode::BAD_REQUEST,
                json!({ "message": "request validation failed", "details": fields }),
                None,
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                json!({ "message": "resource not found" }),
                None,
            ),
            Self::Conflict(code) => (
                StatusCode::CONFLICT,
                json!({ "message": "conflict", "code": code }),
                None,
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                json!({ "message": "authentication required" }),
                None,
            ),
            Self::ForbiddenOrigin => (
                StatusCode::FORBIDDEN,
                json!({ "message": "origin not allowed" }),
                None,
            ),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({ "message": "payload too large" }),
                None,
            ),
            Self::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                json!({ "message": "request timed out" }),
                None,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({ "message": "service unavailable" }),
                None,
            ),
            Self::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "message": "internal error" }),
                Some(format!("{err:#}")),
            ),
            Self::Database(err) => {
                if let sqlx::Error::RowNotFound = err {
                    return (
                        StatusCode::NOT_FOUND,
                        json!({ "message": "resource not found" }),
                        None,
                    );
                }
                if let Some(db) = err.as_database_error()
                    && db.code().as_deref() == Some("23505")
                {
                    let code = if db.constraint().is_some_and(|c| c.contains("username")) {
                        "username_taken"
                    } else {
                        "unique_violation"
                    };
                    return (
                        StatusCode::CONFLICT,
                        json!({ "message": "conflict", "code": code }),
                        None,
                    );
                }
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "message": "internal error" }),
                    Some(err.to_string()),
                )
            }
        }
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        match &rejection {
            JsonRejection::BytesRejection(_) => Self::PayloadTooLarge,
            JsonRejection::JsonSyntaxError(_) => {
                Self::Validation(vec!["body must be well-formed JSON".into()])
            }
            JsonRejection::JsonDataError(_) => {
                Self::Validation(vec!["body does not match the expected schema".into()])
            }
            JsonRejection::MissingJsonContentType(_) => {
                Self::Validation(vec!["content-type must be application/json".into()])
            }
            other => Self::Validation(vec![other.body_text()]),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body, cause) = self.parts();
        let mut res = (status, Body::empty()).into_response();
        res.extensions_mut().insert(EnvelopeMeta { body, cause });
        res
    }
}

/// JSON body extractor whose rejections land in the standard envelope.
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        AxumJson::<T>::from_request(req, state)
            .await
            .map(|j| Json(j.0))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_errors_keep_their_detail() {
        let (status, body, cause) = AppError::Validation(vec!["too short".into()]).parts();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["details"][0], "too short");
        assert!(cause.is_none());

        let (status, body, _) = AppError::Conflict("username_taken").parts();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["code"], "username_taken");
    }

    #[test]
    fn internal_errors_log_the_cause_but_hide_it() {
        let err = AppError::Internal(anyhow::anyhow!("connection refused to shard 7"));
        let (status, body, cause) = err.parts();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "message": "internal error" }));
        assert_eq!(cause.as_deref(), Some("connection refused to shard 7"));
    }

    #[test]
    fn database_errors_route_by_code() {
        let not_found = AppError::Database(sqlx::Error::RowNotFound).parts();
        assert_eq!(not_found.0, StatusCode::NOT_FOUND);

        let db_err = AppError::Database(sqlx::Error::Io(std::io::Error::other("gone")));
        let (status, _, cause) = db_err.parts();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(cause.is_some());
    }

    #[test]
    fn into_response_carries_envelope_meta() {
        let res = AppError::NotFound.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let meta = res.extensions().get::<EnvelopeMeta>().expect("meta");
        assert_eq!(meta.body["message"], "resource not found");
    }
}
