#!/usr/bin/env bash
# `northstack remove-example` (`just remove-example`): delete the example
# "notes" app so you start from an empty app. The tooling and frontend shell
# stay. Runs in the app root; $NORTHSTACK is the northstack CLI.
#
#   remove-example           refuses if git has uncommitted changes
#   remove-example --force   skip that check
#
# Afterwards it resets the dev database and runs `just check`, which rewrites
# every generated file for the empty app.

set -euo pipefail
: "${NORTHSTACK:?run this through the northstack CLI}"

# Homebrew's rustup is keg-only; find it even if it isn't on PATH.
if ! command -v cargo >/dev/null && [[ -x /opt/homebrew/opt/rustup/bin/cargo ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

FORCE=0
[[ ${1:-} == --force ]] && FORCE=1

if [[ ! -d server/src/features/notes ]]; then
  echo "the example is already removed"; exit 0
fi
if [[ $FORCE == 0 ]] && git rev-parse --git-dir >/dev/null 2>&1 \
   && [[ -n $(git status --porcelain) ]]; then
  echo "error: uncommitted changes; commit or stash them first (or pass --force)" >&2
  exit 1
fi

# Backend: the feature, its tests and its migrations.
rm -rf server/src/features/notes server/tests/notes.rs
rm -f server/migrations/*.sql
touch server/migrations/.gitkeep
sed -i.bak '/^pub mod notes;$/d' server/src/features/mod.rs && rm server/src/features/mod.rs.bak
sed -i.bak '/features::notes::/d' server/src/lib.rs && rm server/src/lib.rs.bak

# Frontend: the example pages and component.
rm -rf web/app/notes web/app/components web/e2e/notes.spec.ts
cat > web/app/page.tsx <<'TSX'
// "/": the home page.
export default function Home() {
  return (
    <>
      <title>Home</title>
      <h1>Your app starts here</h1>
      <p className="muted">
        Add a feature with <code>just new-feature &lt;name&gt;</code>, then follow the guide in docs/northstack.md.
      </p>
    </>
  );
}
TSX
sed -i.bak 's|^\( *\)Notes$|\1Home|' web/app/layout.tsx && rm web/app/layout.tsx.bak
sed -i.bak 's|Back to notes|Back home|' web/app/not-found.tsx && rm web/app/not-found.tsx.bak

echo "removed the example app"

# Regenerate everything for the empty app.
if [[ -n ${DATABASE_URL:-} ]] && "$NORTHSTACK" db-reset >/dev/null 2>&1; then
  "$NORTHSTACK" check --no-reload
  echo
  echo "next: just new-feature <name>, then docs/northstack.md -> 'Building a feature, end to end'"
else
  echo "could not reset the database (DATABASE_URL, set in .env by 'just dev'); run 'just db-reset' to see why, then 'just check'"
fi
