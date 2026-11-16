#!/usr/bin/env python3
"""`python3 scripts/changelog_check.py <base>`: a pull request that changes
something apps get must also change crates/northstack/CHANGELOG.md (a line
under `## Unreleased`). CI runs it against the PR's base branch.

"Something apps get": the framework crate (not its tests), or a template file
apps keep a copy of (APP_FILES). Tests, docs, benchmarks and the example
feature need nothing. A pure refactor can skip the check with the PR label
`no changelog` (CI passes --skip).
"""

from __future__ import annotations

import subprocess
import sys

CHANGELOG = "crates/northstack/CHANGELOG.md"
# Files `northstack new` copies that the framework keeps maintaining.
APP_FILES = (
    "Dockerfile", ".dockerignore", "justfile", ".env.example", ".gitignore",
    "clippy.toml", ".cargo/", ".github/workflows/ci.yml", "cli/",
    "server/Cargo.toml", "server/build.rs", "server/src/main.rs", "server/src/lib.rs",
    "server/src/state.rs", "web/package.json", "web/tsconfig.json", "web/vite.config.ts",
    "web/index.html", "web/playwright.config.ts", "web/e2e/app.spec.ts",
)


def app_facing(path: str) -> bool:
    if path.startswith("crates/northstack/"):
        return not path.startswith("crates/northstack/tests/") and path != CHANGELOG
    return any(path == f or (f.endswith("/") and path.startswith(f)) for f in APP_FILES)


def main(base: str) -> int:
    changed = subprocess.run(
        ["git", "diff", "--name-only", f"{base}...HEAD"], capture_output=True, text=True, check=True
    ).stdout.split()
    reaching = [f for f in changed if app_facing(f)]
    if reaching and CHANGELOG not in changed:
        print(
            "error: these changes reach apps, but " + CHANGELOG + " didn't change:\n  "
            + "\n  ".join(reaching)
            + "\n\nAdd a line under '## Unreleased': what changed, then '**Upgrade:**' with the "
            "steps an agent follows in an app (or '**Upgrade:** nothing to do.'). If nothing "
            "reaches apps (a pure refactor), label the PR 'no changelog'.",
            file=sys.stderr,
        )
        return 1
    print("changelog ok")
    return 0


if __name__ == "__main__":
    if "--skip" in sys.argv:
        print("changelog check skipped (label 'no changelog')")
        sys.exit(0)
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        sys.exit(2)
    sys.exit(main(args[0]))
