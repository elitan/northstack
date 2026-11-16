#!/usr/bin/env bash
# `just e2e`: browser smoke tests against the real binary.
#
# Builds the frontend, embeds it in the server binary, starts that binary on
# E2E_PORT (default 3100) with its own database (target/e2e/app.db, reset each run),
# then runs Playwright (web/e2e/*.spec.ts) in headless Chromium.
#
#   just e2e                  all tests
#   just e2e notes            only files matching "notes" (args go to Playwright)

set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null && [[ -x /opt/homebrew/opt/rustup/bin/cargo ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
die() { echo "error: $*" >&2; exit 1; }
: "${DATABASE_URL:?DATABASE_URL is not set (run 'just dev' once, or set it)}"
E2E_DB="sqlite:target/e2e/app.db"
PORT="${E2E_PORT:-3100}"
export E2E_URL="http://localhost:$PORT"
LOG=target/e2e/app.log
mkdir -p target/e2e

(exec 3<>"/dev/tcp/localhost/$PORT") 2>/dev/null && die "port $PORT is in use (set E2E_PORT)"

echo "build    frontend + binary"
(cd web && bun run --silent build >/dev/null) || die "frontend build failed (run 'just check')"
cargo build -q -p server
# Compile against the dev database (sqlx checks queries); run against the e2e one.
export DATABASE_URL="$E2E_DB"
cargo run -q -p northstack --bin northstack -- db-reset >/dev/null

# Downloads Chromium on the first run; a no-op after that.
(cd web && bunx playwright install chromium >/dev/null)

echo "start    $E2E_URL (log: $LOG)"
PORT=$PORT RUST_LOG=warn target/debug/app >"$LOG" 2>&1 &
APP=$!
trap 'kill $APP 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do
  curl -fs "$E2E_URL/readyz" >/dev/null && break
  kill -0 $APP 2>/dev/null || { cat "$LOG" >&2; die "the app exited on start"; }
  sleep 0.5
done
curl -fs "$E2E_URL/readyz" >/dev/null || { cat "$LOG" >&2; die "the app didn't become ready"; }

echo "test     playwright"
if ! (cd web && bunx playwright test "$@"); then
  echo
  echo "app log (last 30 lines of $LOG):"
  tail -30 "$LOG"
  echo
  echo "how to fix: A page doesn't behave as the test expects. Read the failure above;"
  echo "a trace is in web/test-results/ ('cd web && bunx playwright show-trace <file>')."
  echo "Fix the app, or update web/e2e/ if the behavior changed on purpose."
  exit 1
fi
