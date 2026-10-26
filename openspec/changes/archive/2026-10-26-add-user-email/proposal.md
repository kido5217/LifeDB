# Proposal

## Why

`users` currently stores identity alone (username, password hash). Every future recovery or contact flow — email verification, password reset, notifications — needs an address on the account, and deciding that surface now (one additive migration, one API field) prevents a second identity-style migration later. The shape was decided in a 2026-10-26 exploration against the PostgreSQL 18 docs (native `uuidv7()`, citext) and current auth-schema practice: email is a recovery/contact handle, not a login identifier; capture it case-insensitively-unique today, defer the verification machinery.

## What Changes

- Migration `0002_user_email.sql`: enable the `citext` extension; add `users.email citext NULL` with a partial unique index; switch the `id` column default from `gen_random_uuid()` to PostgreSQL 18's native `uuidv7()`.
- Registration now requires an `email` field on a new register-only DTO (validated, max 254 chars); the stored value preserves the input case, and uniqueness is case-insensitive by citext semantics. Login's request shape is unchanged.
- `GET /api/v1/auth/me` and the OpenAPI `MeResponse`/register schemas expose `email`.
- Explicitly deferred (recorded as decided for later changes): email verification + change flow (with the purpose-keyed `auth_tokens` table whose shape is already agreed in design.md), `email_verified_at`, `updated_at`, email as a login identifier, password reset.
- No behavior change to login, logout, or me's session semantics; existing dev rows survive (column is nullable, no backfill).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `user-auth`: registration accepts and requires an email; the account gains a case-insensitive-unique email identity requirement.

## Impact

- Modified: `migrations/` (new 0002), `src/users/{dto,queries,handlers}.rs` (register-only DTO + email, insert/select columns), `src/error.rs` (constraint-name mapping gains `uq_users_email`), `tests/auth.rs` helpers that POST register payloads, `README.md` (register payload example added).
- Database: additive-only; `sqlx::migrate!` applies at next boot; compile-time-checked macros re-validate against the dev DB automatically.
- Dependencies: none new (citext ships with the dev-shell PostgreSQL 18; `validator`'s email rule already available).
