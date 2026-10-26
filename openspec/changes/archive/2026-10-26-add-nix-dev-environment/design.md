# Design

## Context

See `proposal.md` for motivation. Constraints established during exploration:

- Target host is NixOS with flakes; nothing else may be assumed about the host.
- nixpkgs PostgreSQL defaults its unix socket to `/run/postgresql`, which is not user-writable — a project-local socket directory is mandatory (observed as `FATAL: could not create lock file` in community reports).
- Verified this session: `postgresql_18` is 18.6 on `nixpkgs-unstable` and 18.4 on `nixos-25.11`; `just` and `rust-analyzer` exist in the dev-shell package sets; no listener occupies 5432/5433 today.
- Reference pattern for shellHook-managed Postgres: `writeShellScriptBin` lifecycle scripts + idempotent start, per the Phoenix/PostgreSQL flake example and the NixOS Discourse thread.

## Goals / Non-Goals

**Goals:**
- One entry point (`nix develop`, or `just shell` where a host `just` exists) yields a fully working environment including a running database.
- The Postgres lifecycle is crash-safe and re-entrant: shell entry never fails because of the database state.
- All version pinning lives in `flake.lock`; refreshing is a first-class task.

**Non-Goals:**
- No application skeleton, migrations, or ORM choice (later changes).
- No process supervision layer (devenv/process-compose/systemd) — idempotent scripts cover the dev use case.
- No cross-system (aarch64/darwin) support beyond what falls out for free; x86_64-linux is the contract.

## Decisions

### 1. Pin `nixpkgs` to `nixos-unstable`
Latest PostgreSQL patch level (18.6) and latest `just`; freshness matters more than channel calm for a personal project, and `just update` makes refresh explicit. *Alternative:* `nixos-25.11` (slower drift, older packages) — rejected; revisit only if churn bites.

### 2. Rust via oxalica `rust-overlay` → `rust-bin.stable.latest.default.override { extensions = [ "rust-analyzer" ] }`
Version-pinned, pure (hashes pre-fetched in-tree), and the ecosystem standard for Rust dev shells; `follows = "nixpkgs"` avoids duplicate inputs. *Alternatives:* plain `pkgs.rustc`/`pkgs.cargo` (coupled to channel, no component control) and `rustup` wrapper (reintroduces impure toolchain downloads) — both rejected. The `default` profile already ships clippy and rustfmt.

### 3. Postgres as a lifecycle script, shellHook only calls it
`pg-start`/`pg-stop`/`pg-status`/`pg-reset` are `pkgs.writeShellScriptBin` commands in the shell; `shellHook` exports env and calls `pg-start` once. Keeps the hook itself one line and lets `just` tasks and the hook share one implementation. *Alternative:* everything inline in `shellHook` — duplicated logic between hook and tasks; rejected.

### 4. Persistent cluster in `.dev/postgres`, dedicated port 5433, `trust` auth
`PGDATA=$PWD/.dev/postgres`, `unix_socket_directories=$PGDATA`, pid file in `$PGDATA`, TCP on 5433 (`PGPORT=5433`), database `lifedb`, superuser `postgres`, `trust` auth. Persistent beats ephemeral `pg_tmp` because LifeDB dev data has value across sessions; dedicated port immunizes against a future system service (spec scenario). `.dev/` goes in `.gitignore`.

### 5. Idempotent, fast `pg-start`
Check `pg_isready -h $PGDATA -p 5433` first and exit 0 if up; else, if `PGDATA` is missing, `initdb` (with `LANG=C.UTF-8` exported by the shell env to avoid the known locale failure), `pg_ctl start`, then `createdb lifedb` if absent. Re-entry cost is one `pg_isready` (~ms). Recovery from unclean shutdown is `postgres` itself (WAL replay on start) — no extra logic.

### 6. `justfile` as the human surface
```
just shell    -> exec nix develop
just update   -> nix flake update
just db-*     -> the pg lifecycle scripts
just default  -> list recipes
```
Recipes call the scripts directly (they exist inside the shell); `just shell`/`just update` also work from a host `just` because they only need `nix`. Documented ordering: `just shell` → work; nothing else to learn.

## Risks / Trade-offs

- [shellHook blocks entry if start hangs] → `pg_ctl start -t` timeout; hook prints the log path and continues non-fatally so the shell is still usable.
- [`db-reset` silently destroys data] → guarded: prompts unless `--force` style second arg; spec accepts data loss as the defined behavior.
- [Server outlives every shell (no supervisor stops it)] → acceptable for dev; `just db-stop` is documented, and stale postmaster is reclaimed on next `pg-start` via pidfile check.
- [Unstable channel churn breaks `nix develop` mid-project] → `flake.lock` pins the rev; updates only via `just update`.
- [Port 5433 collides with another project] → single-user box today; if it bites, one variable in the flake changes the port (contract stays "exported URL").

## Migration Plan

Pure addition: add files, verify by entering the shell; rollback is deleting `flake.nix`, `flake.lock`, `justfile` and `.dev/`. No deploy, no data migration.

## Open Questions

None blocking — pgvector, migration tooling, and CI build reuse (crane) are deferred to the changes that need them.
