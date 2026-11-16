# Commands for this app. `just` lists them. Each runs the northstack CLI at the
# version in Cargo.toml (`cargo run -p cli`), so they change with `just upgrade`.
# Agents: run `just check` while working and `just verify` before committing.

set dotenv-load := true

ns := "cargo run -q -p cli --"

default:
    @just --list

# Fast loop: migrate, database -> Rust types, compile, Rust -> TS types, typecheck,
# then reload the dev api if `just dev` is running.
check:
    @{{ns}} check

# Everything: check + format + lint + tests + offline query cache + frontend build.
verify:
    @{{ns}} check --full

# CI: like verify, but fails if generated files are stale instead of writing them.
ci:
    @{{ns}} check --ci

# Local dev environment in the background: api on :3000, Vite on :5173 (the
# database is a file, no server). Returns once both are ready. `just check` reloads the api.
# Subcommands: `just dev stop`, `just dev status`, `just dev reload`.
dev *args:
    @{{ns}} dev {{args}}

# Recent warnings and errors from the dev api and Vite (`just logs all`: everything).
logs *args:
    @{{ns}} logs {{args}}

# One-time setup: frontend deps, create and migrate the dev database (a file).
setup:
    cd web && bun install
    @{{ns}} migrate

# Scaffold a new feature module: `just new-feature tags`.
new-feature name:
    @{{ns}} new-feature {{name}}

# Create a new app migration: `just migration add_tags`.
migration name:
    @{{ns}} migration {{name}}

# Apply pending migrations to the dev database.
migrate:
    @{{ns}} migrate

# Delete, recreate and migrate the dev database file (destroys data). Restarts
# the dev api if it's running: it would keep using the deleted file.
db-reset:
    @{{ns}} db-reset

# Delete the example "notes" app and start from an empty app (refuses on
# uncommitted changes; `just remove-example --force` skips that check).
remove-example *args:
    @{{ns}} remove-example {{args}}

# Browser smoke tests: builds the binary, starts it with its own database
# (target/e2e/app.db) and runs web/e2e/*.spec.ts in headless Chromium. CI runs it.
e2e *args:
    @{{ns}} e2e {{args}}

# Move to the latest northstack (or `just upgrade 0.2.0`), print the upgrade
# notes for every version in between, and run `just check`. Commit first.
upgrade *args:
    @{{ns}} upgrade {{args}}

# Release binary with the frontend embedded: target/release/app
build:
    cd web && bun run build
    cargo build --release -p server

# Format Rust code.
fmt:
    cargo fmt --all
