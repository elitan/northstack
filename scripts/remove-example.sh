#!/usr/bin/env bash
# `just remove-example`: delete the example "notes" app so you start from an
# empty app. The framework (crates/northstack), tooling and frontend shell stay.
#
#   scripts/remove-example.sh           refuses if git has uncommitted changes
#   scripts/remove-example.sh --force   skip that check
#
# Afterwards it resets the dev database and runs `just check`, which rewrites
# every generated file for the empty app.

set -euo pipefail
cd "$(dirname "$0")/.."

FORCE=0
[[ ${1:-} == --force ]] && FORCE=1

if [[ ! -d app/src/features/notes ]]; then
  echo "the example is already removed"; exit 0
fi
if [[ $FORCE == 0 ]] && git rev-parse --git-dir >/dev/null 2>&1 \
   && [[ -n $(git status --porcelain) ]]; then
  echo "error: uncommitted changes; commit or stash them first (or pass --force)" >&2
  exit 1
fi

# Backend: the feature, its tests and its migrations.
rm -rf app/src/features/notes app/tests/notes.rs
rm -f app/migrations/*.sql
touch app/migrations/.gitkeep
sed -i.bak '/^pub mod notes;$/d' app/src/features/mod.rs && rm app/src/features/mod.rs.bak
sed -i.bak '/features::notes::/d' app/src/lib.rs && rm app/src/lib.rs.bak

# Frontend: the example pages and component.
rm -f 'web/src/routes/notes.$noteId.tsx' web/src/components/priority.tsx
cat > web/src/routes/index.tsx <<'TSX'
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  component: Home,
});

function Home() {
  return (
    <>
      <h1>Your app starts here</h1>
      <p className="muted">
        Add a feature with <code>just new-feature &lt;name&gt;</code>, then follow the guide in README.md.
      </p>
    </>
  );
}
TSX
sed -i.bak 's|>Notes<|>Home<|; s|^\( *\)Notes$|\1Home|' web/src/routes/__root.tsx && rm web/src/routes/__root.tsx.bak
sed -i.bak 's|Back to notes|Back home|' web/src/components/states.tsx && rm web/src/components/states.tsx.bak

echo "removed the example app"

# Regenerate everything for the empty app.
if [[ -f .env ]]; then set -a; . ./.env; set +a; fi
if [[ -n ${DATABASE_URL:-} ]] && cargo run -q -p northstack --bin northstack -- db-reset >/dev/null 2>&1; then
  ./scripts/check.sh --no-reload
  echo
  echo "next: just new-feature <name>, then README.md -> 'Building a feature, end to end'"
else
  echo "could not reach the database; start it with 'just dev', then run 'just db-reset' and 'just check'"
fi
