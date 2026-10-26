# LifeDB tasks. Database recipes run inside the dev shell (`nix develop` or
# `just shell`); `shell` and `update` work from any shell that has Nix.

default:
    @just --list

# Start the project-local PostgreSQL server (idempotent; the dev shell hook uses the same path).
db-start:
    pg-start

# Stop the project-local PostgreSQL server.
db-stop:
    pg-stop

# Restart the server with data intact.
db-restart:
    pg-restart

# Report server readiness (nonzero exit when down).
db-status:
    pg-status

# Open psql against the exported DATABASE_URL.
db-console:
    exec psql "$DATABASE_URL"

# Destroy all local database data and start a fresh cluster (prompts; add --force to skip).
db-reset *args:
    pg-reset {{args}}

# Create the next SQLx migration file under migrations/ (sequentially numbered).
migrate-create name:
    sqlx migrate add --sequential {{name}}

# Apply pending migrations from migrations/ to DATABASE_URL.
migrate-run:
    sqlx migrate run

# Enter the Nix dev shell.
shell:
    exec nix develop

# Refresh flake inputs (nixpkgs, rust-overlay) and re-lock.
update:
    nix flake update
