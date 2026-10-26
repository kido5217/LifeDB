# Tasks

## 1. Schema

- [x] 1.1 Write `migrations/0002_user_email.sql` exactly as design D1. Verify on the dev cluster: insert one dummy 0001-era row first, run `just migrate-run`, then `psql` (`\d users` + `SELECT` the dummy row) showing the `email citext` column, the `uq_users_email` partial unique index, `uuidv7()` as the new column default, and the pre-existing row intact.

## 2. Persistence + API

- [x] 2.1 Add `email: Option<String>` to the `User` row type and to `find_by_username`'s select; extend `insert` with the email column; verify `cargo check` re-prepares the compile-time macros against the migrated dev DB.
- [x] 2.2 Split DTOs: new `RegisterCredentials { username, password, email }` (`#[validate(email)]`, `length(max = 254)`); `Credentials` (login) unchanged; verify dto unit tests: valid register payload passes, `not-an-email` and >254-char rejected with rule messages; register handler maps validation errors through the existing 400 path.
- [x] 2.3 Expose `email` in `MeResponse` and update the register path attribute's 409 description to "username or email taken"; add `uq_users_email` to `src/error.rs`'s constraint mapping (409, code `unique_violation`); verify integration tests: register->login->me returns the stored original-case email; a legacy row inserted directly via SQL without email logs in and reads `email: null`; error-mapping unit test asserts `conflict_code` for both constraint names, and the integration 409s prove the status end to end.

## 3. Behavior proofs

- [x] 3.1 Verify duplicate-email 409 in BOTH arrival orders: register `x@example.com` then `X@EXAMPLE.com` -> 409, and a second pair reversed -> 409; one row per address in each pair, generic envelope.
- [x] 3.2 Verify email is not a login identifier: login presenting an existing account's email in the username field -> 400 validation envelope (username rule), and no code path queries the email column during login.
- [x] 3.3 Update register call sites in `tests/auth.rs` (and only there — `tests/service.rs` has no register posts); verify full suite green: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.

## 4. OpenAPI + docs

- [x] 4.1 Extend the existing document test in `tests/openapi.rs`: `Credentials` still lacks `email`, `RegisterCredentials` (register request body) and `MeResponse` carry an `email` property, register's 409 description updated; `src/openapi.rs` gains `RegisterCredentials` in the components list.
- [x] 4.2 README: add a short register-example snippet (username, password, email) in the API section; verify its payload matches the test payloads byte-for-byte.

## Workflow follow-up

- Land via the project chain (branch → commit → PR → squash-merge → pull/rebase), then sync the `user-auth` main spec from the delta by hand and `openspec archive add-user-email --skip-specs` (project convention).
