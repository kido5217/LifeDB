//! OpenAPI document (design D9): routes come from the handler
//! `#[utoipa::path]` attributes, schemas from the DTO derives.
//! Served at `/openapi.json`; no browsable UI crate until one is wanted.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::error::ErrorEnvelope;
use crate::users::dto;

/// Cookie-based session security scheme, added as an OpenAPI component.
struct CookieAuth;

impl Modify for CookieAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "sessionCookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("lifedb_session"))),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "LifeDB API",
        version = "1.0.0",
        description = "Browser-first REST API for LifeDB. Session-cookie auth; see /api/v1."
    ),
    paths(),
    components(schemas(
        ErrorEnvelope,
        dto::Credentials,
        dto::RegisterCredentials,
        dto::MeResponse
    )),
    modifiers(&CookieAuth),
    tags((name = "auth", description = "Account registration and session lifecycle")),
)]
pub struct ApiDoc;
