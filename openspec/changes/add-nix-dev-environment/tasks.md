# Tasks

## 1. Flake and dev shell skeleton

- [ ] 1.1 Create `flake.nix`: inputs `nixpkgs` (`nixos-unstable`) and `rust-overlay` (`follows = "nixpkgs"`); `devShells.x86_64-linux.default` via `pkgs.mkShell` with `rust-bin.stable.latest.default.override { extensions = [ "rust-analyzer" ] }`, `postgresql_18`, `just`. Generate `flake.lock`. Verify: `nix flake check` passes and `nix develop --command bash -c 'cargo --version && rust-analyzer --version && psql --version && just --version'` prints tool versions.
- [ ] 1.2 Add `.dev/` to `.gitignore`. Verify: `git check-ignore -v .dev/postgres` matches the rule.

## 2. PostgreSQL lifecycle scripts

- [ ] 2.1 Implement `pg-start` as a `pkgs.writeShellScriptBin` command: short-circuit on `pg_isready -h "$PGDATA" -p 5433`; else `initdb -D "$PGDATA" -A trust -U postgres` (with `LANG=C.UTF-8` in the shell env) when `PGDATA` is absent, then `pg_ctl -D "$PGDATA" -o "-c unix_socket_directories=$PGDATA -c listen_addresses=localhost -p 5433" -l "$PGDATA/postgres.log" -t 30 start`; then `createdb lifedb` if missing. On start timeout, print the log path and exit 0 so the shell remains usable. Verify: with no `.dev/`, one `pg-start` leaves a cluster that answers `psql -h localhost -p 5433 -U postgres -d lifedb -c 'select 1'`; a second `pg-start` returns in well under a second and does not restart the server (compare postmaster pid before/after).
- [ ] 2.2 Implement `pg-stop`, `pg-restart`, `pg-status`, `pg-reset` scripts in the same style (`pg_ctl`/`pg_isready` based; `pg-reset` stops, removes `PGDATA`, then runs `pg-start`). Verify: start → `pg-status` reports ready and exit 0; `pg-stop` → `pg-status` reports down, non-zero exit; `pg-restart` returns ready with data intact; `pg-reset` on a table-populated cluster yields an empty `lifedb`.
- [ ] 2.3 Export from the shell (`env` attrs and/or `shellHook`): `PGDATA=$PWD/.dev/postgres`, `PGPORT=5433`, `PGHOST=$PGDATA`, `DATABASE_URL=postgres://localhost:5433/lifedb`; `shellHook` creates `.dev` parents and calls `pg-start` once. Verify: in a fresh `nix develop`, `psql "$DATABASE_URL" -c 'select 1'` succeeds passwordless, and the second shell entry prints no initdb/start noise.

## 3. justfile and developer documentation

- [ ] 3.1 Write `justfile`: `default` lists recipes; `db-start`, `db-stop`, `db-restart`, `db-status`, `db-console` (`exec psql "$DATABASE_URL"`), `db-reset` delegate to the scripts; `shell` = `exec nix develop`; `update` = `nix flake update`. Verify: plain `just` inside the dev shell lists every recipe with descriptions; `just db-status` and `just db-stop`/`just db-start` round-trip the server; `just update` from a host shell re-solves the lock file without error.
- [ ] 3.2 Add a `Development` section to `README.md`: one-command entry (`just shell`, or `nix develop`), the task list, the `DATABASE_URL` contract, and `.dev/` layout. Verify: every command in the section runs as written from a clean checkout.

## 4. Integration check

- [ ] 4.1 Walk the spec scenarios end to end in one session: unclean-exit recovery (`kill -9` the postmaster, then re-enter the shell — server comes back ready, prior data present); persistence across sessions (`db-stop`, exit, re-enter — data intact); `git status` clean with a running cluster. Record any scenario that fails and fix in place before closing.

## Workflow follow-up

- Land through the protected-main chain from `AGENTS.md`: branch → commit → PR → squash merge → pull/rebase.
- After merge, archive this change (`openspec archive add-nix-dev-environment`) and verify the new `development-environment` spec appears in `openspec list --specs`.
