# LifeDB

## Development

Requires Nix with flakes enabled — nothing else. Enter the environment:

```sh
nix develop   # from the repo root
just shell    # equivalent, if your host already has just (e.g. nix-shell -p just)
```

The dev shell provides the Rust stable toolchain (with `rust-analyzer`),
PostgreSQL 18 (client and server), and `just` itself, all pinned by
`flake.lock`. On entry it automatically initializes and starts a
project-local PostgreSQL cluster stored under `.dev/postgres` (gitignored)
and exports the connection contract:

```sh
DATABASE_URL=postgres://postgres@localhost:5433/lifedb
```

Later application code and any libpq-compatible tool are expected to
connect through this URL only.

Tasks (run inside the dev shell):

| Task | Effect |
| --- | --- |
| `just` | list all tasks |
| `just db-start` / `just db-stop` / `just db-restart` | manage the local server (start is idempotent) |
| `just db-status` | report readiness (nonzero exit when down) |
| `just db-console` | open `psql` against `DATABASE_URL` |
| `just db-reset` | destroy all local data and start a fresh cluster (prompts; `just db-reset --force` to skip) |

From inside the dev shell: `just update` refreshes the pinned flake inputs.

## API development

The service lives in the root Cargo package (`lifedb`). Inside the dev shell
(it starts the database and exports `DATABASE_URL` for you):

```sh
just migrate-run                 # apply migrations/ to the dev database
just migrate-create <name>       # scaffold the next migration file
cargo run                        # boot the API (also applies migrations)
cargo test                       # unit + integration tests (uses lifedb_test)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Once running, the service serves the versioned API under `/api/v1`, plus
`/healthz` (liveness), `/readyz` (readiness, checks the database) and
`/openapi.json` (machine-readable API description).

`cargo run` refuses to start without its required environment:
`DATABASE_URL` arrives from the dev shell, while `ALLOWED_ORIGINS`
is a comma-separated browser-origin allow-list, e.g.
`ALLOWED_ORIGINS=http://localhost:5173 cargo run`. All variables and
defaults are documented in `Config::from_env` in `src/config.rs`.

Accounts register with a username, password and email (email is stored
for recovery/contact; login itself uses the username):

```sh
curl -s -X POST localhost:8080/api/v1/auth/register \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:5173' \
  -d '{"username":"alice_42","password":"twelve chars pass!","email":"alice@example.com"}'
```
