#!/usr/bin/env python3
"""`python3 scripts/release.py <patch|minor|major|X.Y.Z>`: open the release PR.

Run on an up-to-date, clean `main`. It:

1. picks the version: the next patch/minor/major after the latest `v*` tag,
   or X.Y.Z (the first release is the crate's current version);
2. refuses a patch release when **Unreleased** has upgrade steps (apps must
   change something: that's a minor release before 1.0, major after);
3. turns the changelog's **Unreleased** section into the version's section and
   sets the version in crates/northstack/Cargo.toml and Cargo.lock;
4. commits on `release/v<version>`, pushes, and opens a PR with the notes.

Merging that PR releases: .github/workflows/release.yml sees a version on
`main` without a tag, runs all checks, tags it, publishes the crate and
creates the GitHub release.

`--no-push` stops after the commit (to try it locally); `git switch main &&
git branch -D release/v<version>` undoes it.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "crates/northstack"
CHANGELOG = CRATE / "CHANGELOG.md"
MANIFEST = CRATE / "Cargo.toml"
UNRELEASED = "## Unreleased"
VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def die(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def run(*cmd: str, check: bool = True) -> str:
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if check and p.returncode:
        die(f"{' '.join(cmd)} failed:\n{(p.stdout + p.stderr).strip()}")
    return p.stdout.strip()


def key(v: str) -> tuple[int, int, int]:
    m = VERSION.match(v)
    if not m:
        die(f"{v!r} isn't a version (X.Y.Z)")
    return tuple(int(x) for x in m.groups())  # type: ignore[return-value]


def crate_version() -> str:
    return re.search(r'^version = "([^"]+)"', MANIFEST.read_text(), re.M).group(1)


def unreleased(changelog: str) -> tuple[int, int]:
    """Start and end offsets of the Unreleased section's body."""
    start = changelog.find(f"\n{UNRELEASED}\n")
    if start < 0:
        die(f"CHANGELOG.md has no '{UNRELEASED}' heading")
    body = start + len(UNRELEASED) + 2
    end = changelog.find("\n## ", body)
    return body, (len(changelog) if end < 0 else end + 1)


def has_upgrade_steps(notes: str) -> bool:
    """True when any **Upgrade:** line asks apps to do something."""
    for m in re.finditer(r"\*\*Upgrade:\*\*(.*?)(?:\n\n|\Z)", notes, re.S):
        if not re.fullmatch(r"\s*nothing to do\.?\s*", m.group(1), re.I):
            return True
    return False


def next_version(arg: str, latest: str | None, current: str) -> str:
    if latest is None:
        # The first release: the crate's version unless a specific one is given.
        return arg if VERSION.match(arg) else current
    major, minor, patch = key(latest)
    bumps = {
        "patch": f"{major}.{minor}.{patch + 1}",
        "minor": f"{major}.{minor + 1}.0",
        "major": f"{major + 1}.0.0",
    }
    target = bumps.get(arg, arg)
    if key(target) <= key(latest):
        die(f"{target} isn't newer than the latest release {latest}")
    return target


def main(argv: list[str]) -> None:
    args = [a for a in argv if not a.startswith("--")]
    push = "--no-push" not in argv
    if len(args) != 1:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        sys.exit(2)

    # Clean, up-to-date main.
    if run("git", "status", "--porcelain"):
        die("uncommitted changes; a release starts from a clean main")
    if run("git", "branch", "--show-current") != "main":
        die("not on main (git switch main && git pull)")
    run("git", "fetch", "--quiet", "--tags", "origin", "main")
    if run("git", "rev-parse", "HEAD") != run("git", "rev-parse", "origin/main"):
        die("main isn't the same as origin/main (git pull, or push/merge your work first)")
    if push:
        run("gh", "auth", "status")

    tags = [t[1:] for t in run("git", "tag", "--list", "v*").split() if VERSION.match(t[1:])]
    latest = max(tags, key=key) if tags else None
    current = crate_version()
    target = next_version(args[0], latest, current)
    if target in tags:
        die(f"v{target} is already released")

    changelog = CHANGELOG.read_text()
    start, end = unreleased(changelog)
    notes = changelog[start:end].strip()
    if not notes:
        die("nothing under '## Unreleased' in CHANGELOG.md; there is nothing to release")
    if latest and has_upgrade_steps(notes) and key(target)[:2] == key(latest)[:2]:
        die("Unreleased has **Upgrade:** steps (apps must change something); release it as "
            "'minor' (before 1.0) or 'major', not a patch")

    branch = f"release/v{target}"
    print(f"releasing {target} (latest release: {latest or 'none'}) on {branch}")
    run("git", "switch", "--quiet", "-c", branch)

    # "## Unreleased" stays, now empty; the notes move under the new heading.
    rest = changelog[end:]
    CHANGELOG.write_text(
        changelog[:start] + f"\n## {target}\n\n{notes}\n" + (f"\n{rest}" if rest else ""))
    manifest = MANIFEST.read_text()
    MANIFEST.write_text(re.sub(r'^version = "[^"]+"', f'version = "{target}"', manifest, count=1, flags=re.M))
    run("cargo", "update", "--workspace", "--offline", "--quiet")
    body = run(sys.executable, "scripts/release_notes.py", f"v{target}")

    run("git", "add", "-A")
    run("git", "commit", "--quiet", "-m", f"Release v{target}\n\n{body}")
    if not push:
        print(f"committed on {branch} (not pushed). Undo: git switch main && git branch -D {branch}")
        return
    run("git", "push", "--quiet", "-u", "origin", branch)
    pr_body = (f"{body}\n\n---\n\nMerging this releases **v{target}**: the release workflow runs "
               "all checks on `main`, tags the merge commit, publishes the crate to crates.io and "
               "creates the GitHub release with the notes above.")
    url = run("gh", "pr", "create", "--base", "main", "--head", branch,
              "--title", f"Release v{target}", "--body", pr_body)
    print(url)


if __name__ == "__main__":
    main(sys.argv[1:])
