# development-environment Specification

## Purpose

Defines the reproducible LifeDB development environment: what a developer gets when entering the Nix dev shell, how the local PostgreSQL instance behaves across sessions, and which task-runner commands manage both.

## Requirements

### Requirement: Flake-provided dev shell
Entering the development shell via `nix develop` SHALL provide a pinned Rust toolchain, a PostgreSQL 18 server and client, and the `just` command runner, without relying on any of them being installed on the host.

#### Scenario: Bare host can build and connect
- **WHEN** a developer with only Nix (flakes enabled) enters the dev shell on a fresh checkout
- **THEN** `cargo`, `rustc`, `rust-analyzer`, `psql`, `pg_ctl` and `just` are on PATH at versions pinned by the flake lock file

#### Scenario: Host installs are irrelevant
- **WHEN** the host has no Rust, no PostgreSQL and no `just` installed
- **THEN** entering the dev shell still yields a working environment

### Requirement: Automatic idempotent database start
Entering the dev shell SHALL ensure a project-local PostgreSQL server is running and ready to accept connections, and SHALL be safe to run repeatedly without restarting or erroring when one is already up.

#### Scenario: First shell initializes the cluster
- **WHEN** the dev shell is entered and no project-local database exists yet
- **THEN** the database cluster is initialized and started, and the application database is created

#### Scenario: Re-entering while running
- **WHEN** the dev shell is entered while the project-local server is already running
- **THEN** shell entry completes without restarting the server and without error

#### Scenario: Re-entry after an unclean exit
- **WHEN** the dev shell is entered after the server process died without a clean stop
- **THEN** startup recovers the cluster and the server is ready to accept connections

### Requirement: Persistent project-local storage
Database state SHALL persist in a project-local directory under the checkout, MUST be excluded from version control, and MUST NOT be removed by leaving the shell.

#### Scenario: Data survives sessions
- **WHEN** a developer stops the server, exits the shell, and later enters it again
- **THEN** previously committed data is still present

#### Scenario: Working tree stays clean
- **WHEN** database files exist under the data directory
- **THEN** `git status` reports no untracked or modified files caused by them

### Requirement: Fixed connection contract
The dev shell SHALL export a connection URL pointing at the project-local server on a dedicated port with the application database, so tools and later application code connect via a single documented contract.

#### Scenario: Connect via exported URL
- **WHEN** any client inside the dev shell uses the exported connection URL
- **THEN** it reaches the project-local server on port 5433, database `lifedb`, without prompting for a password

#### Scenario: Coexists with a system service
- **WHEN** a system PostgreSQL later runs on the default port 5432
- **THEN** the project-local server still starts and connects on its dedicated port

### Requirement: Task-runner surface
The repository SHALL provide a `justfile` whose default action lists available tasks, including database lifecycle and maintenance tasks: `db-start`, `db-stop`, `db-restart`, `db-status`, `db-console`, `db-reset`, `shell`, and `update`.

#### Scenario: Database lifecycle via tasks
- **WHEN** a developer runs `just db-stop` and then `just db-start`
- **THEN** the server stops and starts again with data intact, and `just db-status` reports the running state

#### Scenario: Reset discards data
- **WHEN** a developer runs `just db-reset`
- **THEN** all local database data is destroyed and a fresh initialized cluster is started

#### Scenario: Discoverability
- **WHEN** a developer runs plain `just`
- **THEN** every task is listed with its description
