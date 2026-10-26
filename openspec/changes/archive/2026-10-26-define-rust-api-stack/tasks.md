# Tasks

## 1. Workspace and toolchain

- [x] 1.1 Create the single Cargo package with the decided dependency set (design D1–D10 versions/features) and a stub `main.rs`; verify `cargo check` succeeds inside `nix develop`.
- [x] 1.2 Extend `.gitignore` for `target/`; verify `git status` stays clean after a `cargo check` run.
- [x] 1.3 Add nixpkgs `sqlx-cli` to the flake devShell packages and `migrate-create` / `migrate-run` recipes to the `justfile` (wrapping `sqlx migrate` against the shell's `DATABASE_URL`); verify `nix develop --command sqlx --version` prints a version and bare `just` lists the new recipes.
- [x] 1.4 Update `README.md` with the API development loop (enter shell, migrate, run, test commands); verify each documented command is exactly the one used in later tasks so a reader following it reproduces the run.

## 2. Persistence and configuration base

- [x] 2.1 Write `migrations/0001_users.sql` for the design D11 `users` table; verify `just migrate-run` applies it on the dev cluster and a fresh test cluster.
- [x] 2.2 Implement `src/config.rs` (`Config::from_env`, all D10 variables, collecting every missing/invalid entry into one error); verify unit tests cover: missing `DATABASE_URL` names it, multiple bad vars all reported together.
- [x] 2.3 Implement `src/db.rs`: pool from config, readiness check helper, and a test helper that creates+migrates `lifedb_test` on the dev cluster; verify an integration test connects to `lifedb_test` and the readiness helper returns ok.
- [x] 2.4 Implement `src/telemetry.rs`: `EnvFilter` + fmt subscriber, pretty vs JSON by config, stdout only; verify a test with JSON format parses a synthetic span's log output and finds method/status/duration fields.

## 3. Service skeleton

- [x] 3.1 Implement `src/error.rs`: `AppError` (thiserror) with the D6 mappings and the `{ message, request_id }` envelope; verify unit tests assert 400/404/409/500 shapes, that 500 bodies carry no internal text, and that the logged cause and envelope request id match.
- [x] 3.2 Implement `src/state.rs` and `src/middleware.rs`: the D7 layer stack in exact order with config-driven timeout/body-limit/origins; verify tests assert: request-id response header present and equal to the value in log/error bodies, oversized body → 413 envelope, foreign-origin preflight → no CORS allow match.
- [x] 3.3 Implement health probes (`/healthz`, `/readyz`) outside the session layers; verify tests: liveness returns 200 with a deliberately broken DB endpoint while readiness returns 503.
- [x] 3.4 Implement `src/main.rs` boot sequence (config → telemetry → pool → `sqlx::migrate!` → serve with graceful shutdown); verify by starting the server against the dev cluster, curling `/healthz` and `/readyz` for 200, sending SIGTERM mid-request and observing completion plus exit 0.

## 4. Auth vertical

- [x] 4.1 Implement `src/auth.rs`: argon2id hash/verify with PHC strings and a precomputed dummy hash; verify unit tests: round-trip verifies, wrong password rejects, stored hash parses as an `$argon2id$` PHC string.
- [x] 4.2 Implement `src/users/` queries (insert, find-by-username) with compile-checked macros; verify tests: duplicate username surfaces the unique-violation variant, not-found maps to the auth 401 path.
- [x] 4.3 Implement register/login handlers and routes with D11 validation policy (validator DTOs); verify integration tests: valid register → success without session cookie, duplicate → 409, weak password → 400 listing failed rules, correct login → 401→200 flow with `HttpOnly`+`SameSite=Lax` cookie, `ENVIRONMENT=prod` adds `Secure`.
- [x] 4.4 Wire `tower-sessions` (memory store, D3, TTL from config); verify integration tests: `/api/v1/auth/me` 200 with session, 401 on tampered/expired cookie, logout expires the cookie and the same cookie value 401s afterward, a freshly built router rejects a pre-restart cookie (restart-revocation scenario).
- [x] 4.5 Implement the Origin check on mutating requests (spec exemption rules for login/register included); verify tests: foreign `Origin` POST with session cookie → 403, same-origin passes, login/register from foreign origin → 403.
- [x] 4.6 Make the unknown-user login path hash-verify against the dummy hash; verify a smoke test with a loose wall-time bound shows unknown-user and wrong-password responses are within one order of magnitude, and both are byte-identical 401 envelopes.

## 5. OpenAPI surface and cross-cutting verification

- [x] 5.1 Implement `src/openapi.rs` (utoipa + utoipa-axum registration of all routes/DTOs) and serve `/openapi.json`; verify the document is valid JSON with every registered `/api/v1` path and its request/response schemas present.
- [x] 5.2 Trace every requirement scenario in both spec deltas to at least one test; verify by listing scenario → test-name pairs in the PR body, closing the loop or adding the missing test.
- [x] 5.3 Final gate: verify `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` all pass in the dev shell; fix anything this change introduced and report pre-existing issues separately.

## Workflow follow-up

- Land via the project chain: branch → commit → PR → squash-merge → pull/rebase (AGENTS.md).
- After merge, write the two main specs from the deltas by hand and archive with `openspec archive define-rust-api-stack --skip-specs` (project convention).
