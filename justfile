# northstack task runner. `just` lists recipes.
# Agents: run `just check` while working and `just verify` before committing.

set dotenv-load := true

default:
    @just --list

# Fast loop: migrate, Postgres -> Rust types, compile, Rust -> TS types, typecheck.
check:
    @./scripts/check.sh

# Everything: check + format + lint + tests + offline query cache + frontend build.
verify:
    @./scripts/check.sh --full

# CI: like verify, but fails if generated files are stale instead of writing them.
ci:
    @./scripts/check.sh --ci

# One-time setup: frontend deps, create and migrate the dev database.
setup:
    cd web && bun install
    cargo run -q -p northstack --bin northstack -- migrate

# Create a new app migration: `just migration add_tags`.
migration name:
    @cargo run -q -p northstack --bin northstack -- migration {{name}}

# Drop, recreate and migrate the dev database (destroys data).
db-reset:
    cargo run -q -p northstack --bin northstack -- db-reset

# Scaffold a new feature module: `just new-feature tags`.
new-feature name:
    ./scripts/new-feature.sh {{name}}

# Run the backend (http + workers + cron) on :3000.
dev:
    cargo run -p app

# Run the Vite dev server on :5173 (proxies /rpc to :3000).
web-dev:
    cd web && bun run dev

# Release binary with the frontend embedded: target/release/app
build:
    cd web && bun run build
    cargo build --release -p app

# Format Rust code.
fmt:
    cargo fmt --all
