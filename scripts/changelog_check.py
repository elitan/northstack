#!/usr/bin/env python3
"""`python3 scripts/changelog_check.py <base>`: changelog rules for a pull
request (CI runs it against the PR's base branch).

1. A change that reaches apps adds a line under `## Unreleased` in
   crates/northstack/CHANGELOG.md: the framework crate (not its tests), or a
   template file apps keep a copy of (see APP_FILES). PRs that don't affect apps
   at all (tests, docs, the example feature, benchmarks) need nothing; a pure
   refactor of the crate can skip it with the PR label `no changelog`.
2. Released sections are history: apps have already seen them. They never
   change, and a new version section only appears together with a new crate
   version (a release PR from scripts/release.py).
"""

from __future__ import annotations

import re
import subprocess
import sys

CHANGELOG = "crates/northstack/CHANGELOG.md"
MANIFEST = "crates/northstack/Cargo.toml"
# Files `northstack new` copies that the framework keeps changing: apps don't
# get updates to them, so a change needs upgrade steps.
APP_FILES = (
    "Dockerfile", ".dockerignore", "justfile", ".env.example", ".gitignore",
    "clippy.toml", ".cargo/", ".github/workflows/ci.yml", "cli/",
    "server/Cargo.toml", "server/build.rs", "server/src/main.rs", "server/src/lib.rs",
    "server/src/state.rs", "web/package.json", "web/tsconfig.json", "web/vite.config.ts",
    "web/index.html", "web/playwright.config.ts", "web/e2e/app.spec.ts",
)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def show(rev: str, path: str) -> str:
    """The file at `rev`, or "" if it didn't exist there."""
    p = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
    return p.stdout if p.returncode == 0 else ""


def sections(changelog: str) -> dict[str, str]:
    out = {}
    for section in changelog.split("\n## ")[1:]:
        heading, _, body = section.partition("\n")
        out[heading.strip()] = body.strip()
    return out


def version(manifest: str) -> str:
    return re.search(r'^version = "([^"]+)"', manifest, re.M).group(1)


def app_facing(path: str) -> bool:
    if path.startswith("crates/northstack/"):
        return not path.startswith("crates/northstack/tests/") and path != CHANGELOG
    return any(path == f or (f.endswith("/") and path.startswith(f)) for f in APP_FILES)


def main(base: str, skip_label: bool) -> int:
    merge_base = git("merge-base", base, "HEAD").strip()
    changed = git("diff", "--name-only", merge_base, "HEAD").split()
    old = sections(show(merge_base, CHANGELOG)) if CHANGELOG in changed else None
    new = sections(open(CHANGELOG).read())
    errors = []

    reaching = [f for f in changed if app_facing(f)]
    unreleased_changed = old is not None and old.get("Unreleased") != new.get("Unreleased")
    new_versions = [v for v in new if v != "Unreleased" and old is not None and v not in old]
    if reaching and not (unreleased_changed or new_versions) and not skip_label:
        errors.append(
            "these changes reach apps, but '## Unreleased' in " + CHANGELOG + " has no new line:\n  "
            + "\n  ".join(reaching)
            + "\nAdd what changed, and '**Upgrade:**' steps an agent can follow in an app (or "
            "'**Upgrade:** nothing to do.'). If nothing reaches apps (a pure refactor), label the "
            "PR 'no changelog'.")

    if old is not None:
        for v, body in old.items():
            if v != "Unreleased" and new.get(v) != body:
                errors.append(f"the released section '## {v}' changed; released notes never change "
                              "(add a line under '## Unreleased' instead)")
        if new_versions:
            old_version = version(show(merge_base, MANIFEST))
            if version(open(MANIFEST).read()) == old_version:
                errors.append(f"new section(s) {new_versions} without a new crate version; "
                              "releases come from 'python3 scripts/release.py'")

    for e in errors:
        print(f"error: {e}\n", file=sys.stderr)
    if not errors:
        print("changelog ok" + (f" ({len(reaching)} app-facing files)" if reaching else ""))
    return 1 if errors else 0


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        sys.exit(2)
    sys.exit(main(args[0], "--skip" in sys.argv))
