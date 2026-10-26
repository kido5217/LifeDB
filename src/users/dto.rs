//! Request/response DTOs with the D11 validation policy.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::{Validate, ValidationError};

/// Login payload: username 3-32 chars of `[a-z0-9_]`, password 12-1024
/// chars (the cap bounds hashing cost).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct Credentials {
    #[validate(
        length(min = 3, max = 32, message = "username must be 3-32 characters"),
        custom(
            function = "check_username_charset",
            message = "username may use a-z, 0-9 and _"
        )
    )]
    pub username: String,
    #[validate(length(min = 12, max = 1024, message = "password must be 12-1024 characters"))]
    pub password: String,
}

/// Registration payload: login credentials plus a required email
/// (add-user-email D2). Kept separate from `Credentials` so login bodies
/// stay exactly `{username, password}`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterCredentials {
    #[validate(
        length(min = 3, max = 32, message = "username must be 3-32 characters"),
        custom(
            function = "check_username_charset",
            message = "username may use a-z, 0-9 and _"
        )
    )]
    pub username: String,
    #[validate(length(min = 12, max = 1024, message = "password must be 12-1024 characters"))]
    pub password: String,
    #[validate(
        length(max = 254, message = "email must be at most 254 characters"),
        email(message = "must be a valid email address")
    )]
    pub email: String,
}

fn check_username_charset(value: &str) -> Result<(), ValidationError> {
    if value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Ok(())
    } else {
        Err(ValidationError::new("username_charset"))
    }
}

/// Rendered by `GET /api/v1/auth/me` (also the row projection).
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    pub username: String,
    /// Null for accounts that predate email capture.
    pub email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(username: &str, password: &str) -> Credentials {
        Credentials {
            username: username.into(),
            password: password.into(),
        }
    }

    fn reg(username: &str, password: &str, email: &str) -> RegisterCredentials {
        RegisterCredentials {
            username: username.into(),
            password: password.into(),
            email: email.into(),
        }
    }

    #[test]
    fn register_accepts_a_clean_triple() {
        assert!(
            reg("alice_42", "twenty chars or more!!", "Alice@Example.com")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn register_rejects_bad_email() {
        assert!(
            reg("alice_42", "twenty chars or more!!", "not-an-email")
                .validate()
                .is_err()
        );
        let too_long = format!("{}@example.com", "a".repeat(243)); // 255 chars
        let err = reg("alice_42", "twenty chars or more!!", &too_long)
            .validate()
            .unwrap_err();
        let messages: Vec<String> = err
            .field_errors()
            .into_values()
            .flat_map(|errors| {
                errors
                    .iter()
                    .map(|e| e.message.clone().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(messages.iter().any(|m| m.contains("254")), "{messages:?}");
    }

    #[test]
    fn accepts_a_clean_pair() {
        assert!(
            creds("alice_42", "twenty chars or more!!")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn rejects_short_password_with_rule_detail() {
        let err = creds("alice", "too short").validate().unwrap_err();
        let messages: Vec<String> = err
            .field_errors()
            .into_values()
            .flat_map(|errors| {
                errors
                    .iter()
                    .map(|e| e.message.clone().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            messages.iter().any(|m| m.contains("12-1024")),
            "{messages:?}"
        );
    }

    #[test]
    fn rejects_bad_charset_and_length_usernames() {
        assert!(creds("Alice", "twelve chars pass!").validate().is_err());
        assert!(creds("ab", "twelve chars pass!").validate().is_err());
        assert!(
            creds(&"a".repeat(33), "twelve chars pass!")
                .validate()
                .is_err()
        );
    }
}
