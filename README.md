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
