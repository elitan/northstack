# northstack task runner. `just` lists recipes.
# Agents: run `just check` while working and `just verify` before committing.

set dotenv-load := true

default:
    @just --list

# Fast loop: migrate, database -> Rust types, compile, Rust -> TS types, typecheck,
# then reload the dev api if `just dev` is running.
check:
    @./scripts/check.sh

# Everything: check + format + lint + tests + offline query cache + frontend build.
verify:
    @./scripts/check.sh --full

# CI: like verify, but fails if generated files are stale instead of writing them.
ci:
    @./scripts/check.sh --ci

# One-time setup: frontend deps, create and migrate the dev database (a file).
setup:
    cd web && bun install
    cargo run -q -p northstack --bin northstack -- migrate

# Prove the README is true: docs paths exist, and the "Building a feature"
# walkthrough runs as written in a temporary copy (own database). CI runs it.
readme-test:
    @python3 scripts/readme_test.py

# Delete the example "notes" app and start from an empty app (refuses on
# uncommitted changes; `just remove-example --force` skips that check).
remove-example *args:
    @./scripts/remove-example.sh {{args}}

# Create a new app migration: `just migration add_tags`.
migration name:
    @cargo run -q -p northstack --bin northstack -- migration {{name}}

# Delete, recreate and migrate the dev database file (destroys data). Restarts
# the dev api if it's running: it would keep using the deleted file.
db-reset:
    cargo run -q -p northstack --bin northstack -- db-reset
    @if [ -f .dev/active ]; then ./scripts/dev.sh reload; fi

# Scaffold a new feature module: `just new-feature tags`.
new-feature name:
    ./scripts/new-feature.sh {{name}}

# Local dev environment in the background: api on :3000, Vite on :5173 (the
# database is a file, no server). Returns once both are ready. `just check` reloads the api.
# Subcommands: `just dev stop`, `just dev status`, `just dev reload`.
dev *args:
    @./scripts/dev.sh {{args}}

# Recent warnings and errors from the dev api and Vite (`just logs all`: everything).
logs *args:
    @./scripts/dev.sh logs {{args}}

# Release binary with the frontend embedded: target/release/app
build:
    cd web && bun run build
    cargo build --release -p server

# Format Rust code.
fmt:
    cargo fmt --all

# Browser smoke tests: builds the binary, starts it with its own database
# (target/e2e/app.db) and runs web/e2e/*.spec.ts in headless Chromium. CI runs it.
e2e *args:
    @./scripts/e2e.sh {{args}}
