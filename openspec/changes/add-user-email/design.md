# Design

## Context

`users` exists since define-rust-api-stack (D11): `id uuid` default `gen_random_uuid()`, `username text UNIQUE COLLATE "C"`, `password_hash text`, `created_at timestamptz`. The register DTO enforces `^[a-z0-9_]{3,32}$`, so every stored username is already lowercase — the case-sensitivity of `"C"` collation is currently harmless. This change adds the email column per the 2026-10-26 exploration (cards 1-6: recovery-contact role, capture-only, nullable, updated_at skipped) and switches the id default to PG 18 native `uuidv7()`. Facts verified against postgresql.org citext docs and 18.0 release notes this session.

## Goals / Non-Goals

**Goals:**
- Email on the account, case-insensitive-unique, required at the API boundary.
- One additive migration; dev rows survive; no backfill.

**Non-Goals:**
- Verification/change/reset flows, `email_verified_at`, gating, `updated_at`, email-as-login, a mailer. All decided-when-to-do entries below are recorded so a later change copies, not redesigns.

## Decisions

### D1: Migration `0002_user_email.sql`
```sql
CREATE EXTENSION IF NOT EXISTS citext;           -- trusted; superuser dev role can install
ALTER TABLE users ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE users ADD COLUMN email citext;
CREATE UNIQUE INDEX uq_users_email ON users (email) WHERE email IS NOT NULL;
```
Nullable column + partial unique index (card 4): rows predating the column stay valid; future OAuth-only or no-email accounts remain representable. The partial index is citext-aware automatically. No collation change to `username` (validator guarantees lowercase; `"C"` kept for cheap exact-match lookups).

### D2: API surface
- Register gains its own DTO, `RegisterCredentials { username, password, email }` (email: `#[validate(email)]` + `length(max = 254)`). Login keeps `Credentials { username, password }` unchanged — the shared DTO today means a required `email` on both routes would 400 every existing login; the split is the fix.
- `register` inserts `(id, username, password_hash, email)`; duplicate username (constraint `users_username_key` -> code `username_taken`) and duplicate email (partial index `uq_users_email` -> code `unique_violation`) are both 409; the `code` field distinguishes which. `src/error.rs`'s constraint-name mapping gains the new index name.
- `me` returns `MeResponse { id, username, email }` — email `null` for legacy rows; the utoipa `ToSchema` derive propagates the new field without touching `openapi.rs`; the register path-attribute's 409 description becomes "username or email taken".
- Login/origin/timeout/etc. untouched.

### D3: Case handling
Store as typed (citext preserves input case, folds only on comparison), matching the citext docs' documented behavior; no app-side lowercasing. Normalization beyond case (plus-addressing, Gmail dots) deliberately NOT done — personal-app scope, single-source-of-truth is the user's own input.

### D4: Decided-for-later (record so the verify-flow change starts pre-agreed)
- `auth_tokens` table: `id uuid PK DEFAULT uuidv7()`, `user_id` FK ON DELETE CASCADE, `purpose text CHECK IN ('email_verify','email_change','password_reset')`, `target_email citext NULL`, `token_hash text NOT NULL` (SHA-256 of the raw hex token; raw never stored), `created_at`, `expires_at` (24h verify / 1h change / 1h reset). Single-use via `DELETE ... WHERE token_hash = $1 RETURNING *` inside the consuming transaction (anti-replay pattern converged on by better-auth and the learn-repo migration).
- Verification is informational (no login gate); reset sends only to verified emails; change keeps the old email live until the new one verifies; `pending_email` lives in the token row (`target_email`), not as a users column.
- `email_verified_at timestamptz NULL` and (only if audit value appears) `updated_at` with a `BEFORE UPDATE` trigger: separate additive migrations.
- Login resolution stays username-only; revisit email-as-login only if multi-owner use materializes (then: username-first, activated-email fallback, and the shadowing guard from the arz.ai pattern).

## Risks / Trade-offs

- [citext folds per DB `LC_CTYPE`, not full Unicode case-folding] — acceptable for ASCII/typical Latin emails; official docs' own caveat. Nondeterministic ICU collation is the stricter alternative but adds collation-versioning complexity now for no gain.
- [Two 409 sources (username, email) carry different `code` values] — `username_taken` vs generic `unique_violation`; consistent with the existing mapping's intent, and the register response remains body-less 409 + envelope. If a later UX wants symmetry, rename codes then.
- [`uuidv7()` default change only affects rows inserted outside the app] — app always supplies ids (D8 unchanged); the default is the manual-insert safety net, now a better one.
- [Email without verification = self-reported] — capture-only by scope; the column's meaning today is "user-typed address", made honest by deferring trust to the verify change.

## Migration Plan

Apply on next boot (`sqlx::migrate!`) or `just migrate-run`. Rollback within dev only: drop column + index. No data rewrites; the `ALTER COLUMN id SET DEFAULT` is metadata-only.

## Open Questions

- None blocking. (Mailer transport for the verify/reset change is that change's decision.)
