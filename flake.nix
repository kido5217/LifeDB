{
  description = "LifeDB development environment: Rust, PostgreSQL, just";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      # Single source of truth for the project-local database connection.
      pgPort = 5433;
      dbName = "lifedb";

      # Shared prologue for the lifecycle scripts: fail fast, honor the
      # environment exported by the dev shell.
      prelude = ''
        set -euo pipefail
        : "''${PGDATA:?PGDATA must point at the project-local data directory}"
        PGPORT="''${PGPORT:-${toString pgPort}}"
      '';

      pgStart = pkgs.writeShellScriptBin "pg-start" ''
        ${prelude}
        if pg_isready -q -h "$PGDATA" -p "$PGPORT"; then
          exit 0
        fi
        mkdir -p "''${PGDATA%/*}"
        if [ ! -e "$PGDATA/PG_VERSION" ]; then
          if [ -d "$PGDATA" ] && [ -n "$(ls -A "$PGDATA")" ]; then
            echo "pg-start: $PGDATA exists but is not a database cluster; remove it to reinitialize" >&2
            exit 0
          fi
          # Atomic claim: concurrent first entries race, the loser waits below.
          if mkdir "$PGDATA" 2>/dev/null; then
            echo "pg-start: initializing cluster in $PGDATA"
            initdb -D "$PGDATA" -A trust -U postgres
          else
            for _ in $(seq 1 60); do
              sleep 0.5
              [ -e "$PGDATA/PG_VERSION" ] && break
            done
            if [ ! -e "$PGDATA/PG_VERSION" ]; then
              echo "pg-start: another entry is still initializing $PGDATA" >&2
              exit 0
            fi
          fi
        fi
        if ! pg_ctl -D "$PGDATA" -l "$PGDATA/postgres.log" -t 30 \
          -o "-c unix_socket_directories='$PGDATA' -c listen_addresses=localhost -p $PGPORT" \
          start; then
          echo "pg-start: server did not start; see $PGDATA/postgres.log (another cluster may already hold port $PGPORT)" >&2
          exit 0
        fi
        if ! psql -h "$PGDATA" -p "$PGPORT" -U postgres -ltq | cut -d'|' -f1 | grep -qw ${dbName}; then
          createdb -h "$PGDATA" -p "$PGPORT" -U postgres ${dbName}
        fi
      '';

      # NOTE: pg_ctl trusts the pidfile. After kill -9 recovery works (dead
      # pid), but a recycled stale pid can make it signal the wrong process;
      # recover manually: remove "$PGDATA/postmaster.pid", then pg-start.
      pgStop = pkgs.writeShellScriptBin "pg-stop" ''
        ${prelude}
        pg_ctl -D "$PGDATA" -m fast stop
      '';

      pgStatus = pkgs.writeShellScriptBin "pg-status" ''
        ${prelude}
        if pg_isready -q -h "$PGDATA" -p "$PGPORT"; then
          echo "pg-status: ready (data $PGDATA, port $PGPORT, db ${dbName})"
        else
          echo "pg-status: down"
          exit 1
        fi
      '';

      pgRestart = pkgs.writeShellScriptBin "pg-restart" ''
        ${prelude}
        pg_ctl -D "$PGDATA" -m fast stop 2>/dev/null || true
        exec pg-start
      '';

      pgReset = pkgs.writeShellScriptBin "pg-reset" ''
        ${prelude}
        if [ "''${1:-}" != "--force" ] && [ -t 0 ]; then
          printf 'pg-reset: destroy all local data in %s? [y/N] ' "$PGDATA"
          read -r ans
          case "$ans" in
            y|Y) ;;
            *) echo "pg-reset: aborted"; exit 1 ;;
          esac
        fi
        pg_ctl -D "$PGDATA" -m fast stop 2>/dev/null || true
        rm -rf "$PGDATA"
        exec pg-start
      '';

      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-analyzer" ];
      };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          rustToolchain
          postgresql_18
          just
          sqlx-cli
          pgStart
          pgStop
          pgStatus
          pgRestart
          pgReset
        ];

        # initdb and psql misbehave under some host locales on Nix/NixOS.
        LANG = "C.UTF-8";

        shellHook = ''
          # Anchor the cluster to the repo root, not the entry directory.
          root="$PWD"
          while [ "$root" != / ] && [ ! -e "$root/.git" ]; do root="''${root%/*}"; done
          [ -e "$root/.git" ] || root="$PWD"
          mkdir -p "$root/.dev"
          export PGDATA="$root/.dev/postgres"
          export PGHOST="$PGDATA"
          export PGPORT=${toString pgPort}
          export PGUSER=postgres
          export DATABASE_URL="postgres://postgres@localhost:${toString pgPort}/${dbName}"
          pg-start
        '';
      };
    };
}
