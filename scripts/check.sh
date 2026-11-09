#!/usr/bin/env bash
# The one command agents run. Regenerates everything in the right order and
# checks it, printing only what failed and how to fix it.
#
#   scripts/check.sh           fast loop: migrate, gen-db, compile, gen-ts, typecheck
#   scripts/check.sh --full    + format, lint, tests, offline query cache, frontend build
#   scripts/check.sh --ci      like --full, but verifies generated files instead of
#                              writing them (fails if someone forgot to regenerate)
#
# Exit code 0 means everything passed.

set -uo pipefail
cd "$(dirname "$0")/.."

MODE="check"
for arg in "$@"; do
  case "$arg" in
    --full) MODE="full" ;;
    --ci) MODE="ci" ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [[ -f .env ]]; then set -a; . ./.env; set +a; fi
export CARGO_TERM_COLOR=never NO_COLOR=1

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
START=$(date +%s)
STEP_NO=0

# step <name> <hint> <command...>
step() {
  local name="$1" hint="$2"; shift 2
  STEP_NO=$((STEP_NO + 1))
  local t0=$(date +%s)
  if "$@" >"$LOG" 2>&1; then
    printf 'ok   %-10s %ss\n' "$name" "$(( $(date +%s) - t0 ))"
  else
    printf 'FAIL %-10s %ss\n\n' "$name" "$(( $(date +%s) - t0 ))"
    # Keep the output short: diagnostics only, no progress noise.
    grep -vE '^\s*(Compiling|Checking|Finished|Running|Downloaded|Downloading|Updating|Blocking|Locking|Adding|warning: unused manifest)' "$LOG" \
      | grep -vE '^\s*$' | head -n 80
    local lines; lines=$(wc -l <"$LOG")
    [[ $lines -gt 80 ]] && echo "... ($lines lines total; showing the first 80)"
    printf '\nhow to fix: %s\n' "$hint"
    printf '\nFAILED at step %d (%s) after %ss\n' "$STEP_NO" "$name" "$(( $(date +%s) - START ))"
    exit 1
  fi
}

CI_FLAG=""
[[ $MODE == "ci" ]] && CI_FLAG="--check"

northstack() { cargo run -q -p northstack --bin northstack -- "$@"; }
app() { cargo run -q -p app -- "$@"; }

step migrate \
  "Fix the migration SQL. Never edit a migration that was already applied; add a new one. In development, 'just db-reset' starts over." \
  northstack migrate

step gen-db \
  "Rust types are generated from Postgres into app/src/db/schema.rs. In CI: run 'just check' locally and commit the file. For unsupported types, change the column type." \
  northstack gen-db $CI_FLAG

step compile \
  "Fix the errors above. 'error returned from database' means a query doesn't match the schema; the current schema is in app/src/db/schema.rs. Custom enum columns need 'col AS \"col: EnumType\"' in SELECT and 'value as EnumType' for parameters." \
  cargo build -q -p app --bin app --message-format short

step gen-ts \
  "TypeScript types are generated from Rust into web/src/rpc. In CI: run 'just check' locally and commit the changes." \
  app gen-ts $CI_FLAG

typecheck() {
  (cd web && bunx tsr generate >/dev/null 2>&1 && bunx tsc -b --pretty false)
}
step typecheck \
  "The frontend no longer matches the API or routes. Current API types: web/src/rpc/types/ and web/src/rpc/procedures.ts. Update the React code; never edit generated files." \
  typecheck

if [[ $MODE == "check" ]]; then
  printf '\nall checks passed in %ss (run with --full before committing)\n' "$(( $(date +%s) - START ))"
  exit 0
fi

if [[ $MODE == "ci" ]]; then
  step format "Run 'just fmt' and commit." cargo fmt --all --check
  routes_clean() { git diff --quiet -- web/src/routeTree.gen.ts; }
  step routes "web/src/routeTree.gen.ts is out of date. Run 'just check' and commit." routes_clean
else
  step format "rustfmt failed; fix the syntax error above." cargo fmt --all
fi

step lint \
  "Fix the clippy findings above. Warnings are errors." \
  cargo clippy -q --workspace --all-targets --message-format short -- -D warnings

step test \
  "A test failed. Read the assertion above, fix the code (or the test if the behavior change is intended)." \
  cargo test -q --workspace

if [[ $MODE == "ci" ]]; then
  step sqlx-cache "The offline query cache (.sqlx/) is stale. Run 'just verify' locally and commit .sqlx/." \
    cargo sqlx prepare --workspace --check -- --all-targets
else
  step sqlx-cache "Refreshing .sqlx/ failed; make sure sqlx-cli is installed." \
    cargo sqlx prepare --workspace -- --all-targets
fi

web_build() { (cd web && bunx vite build --logLevel error); }
step build "The frontend build failed; see the error above." web_build

printf '\nall checks passed in %ss\n' "$(( $(date +%s) - START ))"
