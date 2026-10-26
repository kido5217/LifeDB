# Proposal

## Why

LifeDB has no reproducible development environment: every tool (Rust toolchain, PostgreSQL, task runner) currently depends on ad-hoc host installs, and the host today has no `cargo` and no database at all. A Nix flake plus a `just` task runner pins everything the project needs and makes a working setup one `nix develop` away on any machine with Nix.

## What Changes

- Add `flake.nix` / `flake.lock` pinned to `nixos-unstable` with an `oxalica/rust-overlay` input (`follows = "nixpkgs"`).
- Add a `devShells.x86_64-linux.default` providing: Rust stable via `rust-bin.stable.latest.default` with the `rust-analyzer` extension, `postgresql_18` (server + client), and `just`.
- The shellHook starts a project-local PostgreSQL server idempotently: data under `.dev/postgres` (gitignored), `trust` auth, dedicated port 5433, unix socket in the data dir, `DATABASE_URL=postgres://localhost:5433/lifedb` exported; re-entering the shell while the server runs is a no-op.
- Add a `justfile` exposing the lifecycle and workflow commands: `db-start`, `db-stop`, `db-restart`, `db-status`, `db-console`, `db-reset`, `shell`, `update`.
- Extend `.gitignore` for `.dev/`.
- No Cargo project skeleton and no application code — environment only; the first crate and migrations land in later changes.

## Capabilities

### New Capabilities

- `development-environment`: the declarative, self-contained dev shell — toolchain provisioning, database lifecycle (start/stop/status/reset), and the task-runner surface developers interact with.

### Modified Capabilities

_None — no specs exist yet._

## Impact

- New files at repo root: `flake.nix`, `flake.lock`, `justfile`; `.gitignore` gains `.dev/`.
- No code, CI, or deployment impact; nothing that consumes the environment exists yet.
- Host-side footprint: `.dev/` directory per checkout (database data); runtime dependency on Nix with flakes enabled.
