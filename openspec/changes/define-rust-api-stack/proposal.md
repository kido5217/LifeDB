# Proposal

## Why

The repository has no Rust code yet, and the first slice of the backend must land on a settled dependency and architecture foundation rather than ad-hoc choices made mid-implementation. An exploration session (2026-10-26) validated a crate stack against current crates.io releases and 2026 practice and resolved every open fork (sessions over JWT for the browser-only client, logging without OpenTelemetry, SQLx with compile-time-checked queries). This change records those decisions as durable capability specs and delivers the first service slice that embodies them.

## What Changes

- Introduce a Cargo workspace with an Axum + Tokio service binary that connects to the project-local PostgreSQL through the `DATABASE_URL` contract exported by the dev shell.
- New `api-service` capability: fail-fast environment configuration, `sqlx::migrate!` migrations at startup, structured `tracing` logging (pretty in dev, JSON in prod, stdout), a single typed error envelope, the tower-http middleware chain (request-id, trace, timeout, pinned CORS, compression, body limit), `/api/v1` routing, a generated `/openapi.json` via utoipa, `/healthz` + `/readyz` probes, and graceful shutdown.
- New `user-auth` capability: Argon2id password hashing, session login/logout backed by `tower-sessions` (in-memory store for now), `HttpOnly`/`SameSite=Lax`/prod-`Secure` cookies, an Origin check on mutating requests, and a timing-safe login path. First endpoints: register, login, logout, current-user.
- Dev environment additions: `sqlx-cli` (nixpkgs) into the flake dev shell and migration helper recipes in the `justfile`; the existing `development-environment` requirements themselves do not change.
- Explicitly not in this change: OpenTelemetry export, JWT, rate limiting, a browsable API UI, Postgres-backed session store, and the domain (life-record) endpoints themselves. Rationale and upgrade paths live in design.md.

## Capabilities

### New Capabilities

- `api-service`: the REST service binary — startup contract, configuration, logging, error envelope, middleware behavior, versioned routing, OpenAPI document, health probes, and shutdown.
- `user-auth`: account lifecycle for the browser client — password hashing, session issuance, validation and revocation, cookie attributes, and CSRF/origin protection.

### Modified Capabilities

- None. The dev shell gains `sqlx-cli` and `just` recipes, but the existing `development-environment` requirements (toolchain contents, connection contract, task-runner surface) remain satisfied as written; the additions do not modify any requirement statement.

## Impact

- New files: `Cargo.toml` (workspace), service crate under `src/` or `crates/`, `migrations/` (SQLx migration files), `.gitignore` additions for `target/`.
- Modified: `flake.nix` (dev shell package list), `justfile` (migration recipes), `README.md` (dev-loop documentation).
- Dependencies: the decided crate set (axum 0.8, tokio 1, sqlx 0.9, tower/tower-http, tracing/tracing-subscriber, tower-sessions 0.15, argon2 0.6, utoipa 6 + utoipa-axum 0.3, uuid 1, chrono 0.4, validator 0.21, thiserror 2, anyhow 1, serde).
- Runtime coupling: compile-time-checked SQLx macros require the project-local database during in-shell builds (already auto-started by the dev shell); CI will later need either a live PostgreSQL or `SQLX_OFFLINE` prepared data — recorded as a deferred decision.
