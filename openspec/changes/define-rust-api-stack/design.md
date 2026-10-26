# Design

## Context

Greenfield Rust side: the repo currently holds only the Nix dev environment (memory: `nix develop` enters a shell with pinned Rust stable + rust-analyzer, PostgreSQL 18, `just`, an auto-started project-local cluster on port 5433, and `DATABASE_URL=postgres://postgres@localhost:5433/lifedb`). The client is browser-only for now. All dependency versions and ecosystem facts below were verified against crates.io and the cited project docs on 2026-10-26. This document records the how; `proposal.md` holds the why and the spec deltas hold the what.

## Goals / Non-Goals

**Goals:**
- A single service crate that boots, self-checks, migrates, serves `/api/v1`, and shuts down cleanly — the substrate every later feature change builds on.
- A complete, pinned decision record so later changes don't re-litigate the stack.

**Non-Goals:**
- Domain (life-record) endpoints, UI, deployment, CI — later changes.
- Multi-instance operation or horizontal scaling; one process, one Postgres.
- Password reset, email verification, TOTP, OAuth — auth surface stays register/login/logout/me.

## Decisions

### D1: Axum 0.8 + Tokio 1 runtime
Ecosystem default for async REST in Rust; 0.8 is stable-in-practice despite 0.x. Alternative (actix-web) rejected: heavier API, smaller middleware ecosystem than Tower.

### D2: SQLx 0.9 with compile-time-checked macros, not an ORM
We write SQL; the `query!` macros check it against the live project-local DB at compile time — the dev shell's auto-started Postgres and exported `DATABASE_URL` make this free. Alternatives: Diesel (sync core, DSL learning tax), SeaORM 2.0 (rejected now: relations/change-tracking we don't need yet; it layers on SQLx later without a driver swap), deadpool-postgres (fast pool but loses checked macros and adds a second DB stack). Dynamic queries use `QueryBuilder` (unchecked — accepted); if dynamic filtering grows, SeaQuery/SeaORM is the on-ramp. Offline mode (`SQLX_OFFLINE` + prepared data) is the CI escape hatch, deferred until CI exists.

### D3: Sessions over JWT; tower-sessions with the memory store
Browser-only client: an `HttpOnly` `SameSite=Lax` session cookie beats a JWT in localStorage (XSS-resistant, instantly revocable, no refresh machinery). `tower-sessions` 0.15 core is active, but its Postgres store (`tower-sessions-sqlx-store` 0.15) still pins `sqlx ^0.8` and `tower-sessions-core =0.14` — it cannot be used without downgrading SQLx. Decision: use the in-memory store now; the two accepted consequences — restarts log everyone out, single instance only — have a planned exit: when the store (or a custom store on our `PgPool`) catches up, flip the backend with no spec change (revocation semantics are identical; the main spec already mandates restart-revocation only as current behavior). Alternative considered and rejected: a thin hand-rolled session table (~120 lines); still the fallback if the store ecosystem never recovers. CSRF posture: SameSite=Lax plus an `Origin` allow-list check on mutating requests (spec `user-auth`); no token double-submit at this stage.

### D4: Password hashing with argon2 0.6 (RustCrypto)
Argon2id via crate defaults (~19 MiB, t=2, p=1 — OWASP-aligned), PHC self-describing strings for future parameter upgrades. Login path verifies against a precomputed dummy hash when the user is unknown (timing-safe enumeration defense). `jsonwebtoken`/`jwt-compact` dropped with JWT.

### D5: No OpenTelemetry yet; tracing as the one instrumentation surface
`tracing` 0.1 + `tracing-subscriber` 0.3, `EnvFilter` for levels, `fmt` layer pretty in dev / JSON in prod, to stdout (the process manager captures; no self-managed log files). Handlers emit spans with correlation fields from day one, so adding `tracing-opentelemetry` later is additive: OTel traces are still Beta upstream and there is no collector to talk to.

### D6: Error model
`thiserror` 2 `AppError` enum at the web boundary implements `IntoResponse`: maps known variants (validation → 400 with per-field detail, auth → 401/403, not found → 404, unique violation → 409, everything else → 500). 5xx responses log the full cause (with request id) and return the generic envelope only. `anyhow` is allowed in startup/glue/tests but never crosses an HTTP boundary.

### D7: Middleware stack and order (tower-http 0.7)
Outermost → innermost:

```
set-request-id(uuid v7) -> TraceLayer -> TimeoutLayer(30s)
  -> CorsLayer(pinned origins) -> Compression -> DefaultBodyLimit(2 MiB)
    -> session layer -> origin check -> routes
```

Health probes sit inside request-id + trace but outside session/auth so they never require a cookie. Rate limiting (`tower-governor` 0.8) deferred until the login endpoint faces the internet.

### D8: IDs and time
UUID v7 primary keys (`uuid` 1.29) — time-ordered, index-friendly, no count leakage; generated in-app (Postgres `gen_random_uuid()` (v4) kept only as a column default safety net for migrations, application always sends its own id). Timestamps: `chrono` 0.4 with `timestamptz` columns and `Utc`/`Zoned` in code; sqlx and utoipa both integrate with chrono directly, while `jiff` 0.2 has no sqlx integration yet (checked 0.9.0's feature list) — revisit when it does. `sqlx` features: `runtime-tokio`, `tls-rustls`, `postgres`, `macros`, `migrate`, `chrono`, `uuid`, `json`.

### D9: OpenAPI with utoipa 6 + utoipa-axum 0.3
utoipa 6.0 (Sep 2026) requires MSRV 1.88 — satisfied by the flake's `rust-bin.stable.latest`. Serve `/openapi.json` only; no bundled UI crate now (`utoipa-scalar` 0.4 is the one-line upgrade path; `utoipa-swagger-ui` skipped for its vendored-build lag). DTO serde structs are the schema source; `validator` 0.21 derives on the same DTOs run at the extractor boundary.

### D10: Configuration, layout, migrations
One `Config::from_env()` struct, fail-fast, listing all problems at once; no config-file layer (the `config` crate is deferred until file-based settings are real; `envy` is unmaintained — hand-rolled parsing). Variables: `DATABASE_URL`, `LISTEN_ADDR`, `ENVIRONMENT` (dev|prod), `LOG_FORMAT` (pretty|json), `RUST_LOG` (EnvFilter), `ALLOWED_ORIGINS` (comma list), `COOKIE_SECURE` (implied by `ENVIRONMENT=prod`, overridable), `BODY_LIMIT_BYTES`, `REQUEST_TIMEOUT_SECS`, `SESSION_TTL_SECS`. Single crate, module-by-feature:

```
Cargo.toml            # one package; revisit as a workspace when a second appears
src/
  main.rs             # config -> telemetry -> pool -> migrate -> serve(+shutdown)
  config.rs
  telemetry.rs
  error.rs            # AppError + envelope
  state.rs            # AppState { pool, config, sessions }
  db.rs               # pool construction, readiness check
  openapi.rs
  middleware.rs       # layer stack builders, origin check
  users/              # mod.rs (routes), dto.rs, queries.rs, handlers.rs
  auth.rs             # argon2 hash/verify, dummy hash, session helpers
migrations/           # sqlx migrate: 0001_users.sql ...
tests/                # integration tests over the Router via tower::oneshot + live DB
```

Migrations run via `sqlx::migrate!` at startup (embedded) and `sqlx-cli` from the shell; flake adds nixpkgs `sqlx-cli`, `justfile` gains `migrate-create` / `migrate-run` recipes.

### D11: Data model (this change)
`users(id uuid pk, username text unique collate "C", password_hash text, created_at timestamptz default now())`. Username policy: 3–32 chars, `[a-z0-9_]` (validated at DTO level; uniqueness via the constraint → 409). Password policy: ≥ 12 chars, ≤ 1024 (caps argon2 cost). Sessions live in memory only.

### D12: Endpoints (API v1, this change)
`POST /api/v1/auth/register`, `POST /api/v1/auth/login`, `POST /api/v1/auth/logout`, `GET /api/v1/auth/me`, plus `/healthz`, `/readyz`, `/openapi.json`. The surface stays exactly that; later domain changes extend `/api/v1/*`.

### Testing strategy
Integration tests build the `Router`, run it with `tower::ServiceExt::oneshot` against a dedicated test database (same dev cluster, `lifedb_test`, created + migrated by a test helper), and assert every spec scenario that is externally observable: envelope shape, correlation header, probes, 413/timeout, register/login/logout/me flows, enumeration timing smoke check (loose bound, not a flake source), OpenAPI document lists all routes. Compile-time-checked macros run against the live dev DB exactly as in the shell.

## Risks / Trade-offs

- [Memory store: restarts log all users out] → Accepted for single-user browser scope; upgrade path defined in D3 and already required by the spec's restart-revocation scenario, so swapping stores later is not a spec change.
- [SQLx macro compile depends on a live DB] → In-shell builds get it free from the dev shell; CI decision deferred with the `SQLX_OFFLINE` fallback documented (D2).
- [utoipa 6.0 is one month old] → Pinned to 6.x; worst case a 6.x patch; document generation is isolated to DTO annotations and one `openapi.rs`.
- [chrono vs jiff churn] → sqlx feature check says jiff is unintegrable today; D8 keeps the seam thin (chrono only at type boundaries, `timestamptz` in storage).
- [Single crate grows awkward] → Module-by-feature layout was chosen so a `crates/` split is mechanical; no behavior depends on the layout.
- [Layer order mistakes degrade tracing quality] → D7 order is explicit and reviewed at implementation; tests assert request-id presence in logs and error bodies.
- [No rate limit on login] → Deferred until internet-facing; dev/prod single-user exposure is the accepted interim (D7).

## Migration Plan

Forward-only first deployment: `cargo run` applies `sqlx::migrate!`; rollback of the binary is trivial (no data yet). After real data exists, migrations stay append-only (down-migrations not a tooling concern; `just db-reset` covers the dev loop).

## Open Questions

- CI runner shape (live Postgres service vs `SQLX_OFFLINE` prepared data) — must be answered when CI is built; either satisfies D2, no artifact depends on it today.
- Whether `/api/v1/auth/me` returns a stable `user_id` field name vs `id` — cosmetic, settle at DTO review.
