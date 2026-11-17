#!/usr/bin/env python3
"""`python3 scripts/guide_test.py`: prove that `northstack new` works and that
the guide is true. CI runs it.

1. paths:    every path README.md and AGENTS.md mention exists in this repo.
2. new:      `northstack new --local .` creates an app that passes `just check`,
             and every path its docs (AGENTS.md, README.md, docs/northstack.md)
             mention exists in it.
3. example:  `just remove-example` turns it into a clean empty app.
4. guide:    the "Building a feature" walkthrough in crates/northstack/GUIDE.md,
             executed exactly as written. Steps are fenced code blocks preceded
             by a hidden marker:

               <!-- guide-test: run -->                 run the block in bash
               <!-- guide-test: migration <name> -->    `just migration <name>`, write the block to it
               <!-- guide-test: write <path> -->        write the block to <path>

The app lives in a temporary directory with its own database and shares
`target/guide-test` across runs, so it doesn't touch your data and recompiles
incrementally without invalidating your main build.
"""

from __future__ import annotations

import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GUIDE = ROOT / "crates/northstack/GUIDE.md"
MARKER = re.compile(
    r"<!-- guide-test: (\w+)(?: (\S+))? -->\s*\n```[\w-]*\n(.*?)\n```", re.DOTALL
)
START = time.time()


def ok(name: str, t0: float) -> None:
    print(f"ok   {name:<60} {time.time() - t0:.0f}s", flush=True)


def fail(name: str, output: str, hint: str, keep: Path | None = None) -> None:
    print(f"FAIL {name}\n")
    lines = [l for l in output.strip().splitlines() if l.strip() and not l.lstrip().startswith("Compiling ")]
    print("\n".join(lines[-60:]))
    print(f"\nhow to fix: {hint}")
    if keep:
        print(f"\nthe temporary app is kept for inspection: {keep}")
    sys.exit(1)


def missing_paths(root: Path, docs: list[Path], prefixes: tuple[str, ...], app_only: bool = False) -> list[str]:
    """Paths in backticks or markdown links that start with one of `prefixes`
    (or end in .md) and don't exist under `root`."""
    missing = []
    for doc in docs:
        text = doc.read_text()
        # Paths created by the walkthrough itself.
        created = set(m.group(2) for m in MARKER.finditer(text) if m.group(1) == "write")
        candidates = set(re.findall(r"`([^`\s]+)`", text))
        candidates |= set(re.findall(r"\]\(([^)#\s]+)\)", text))
        for c in candidates:
            p = c.rstrip(".,:;")
            if not (p.startswith(prefixes) or p.endswith(".md")) or "://" in p:
                continue
            # <name>, globs and (group) folders are illustrative, not real paths.
            if any(ch in p for ch in "<>*{}()~") or p in created or "/projects" in p:
                continue
            if p.startswith(("web/dist", ".dev", "data/")) or p == "CHANGELOG.md":
                continue
            # Files only apps have (written by `northstack new`).
            if app_only and p in ("docs/northstack.md",):
                continue
            if not (root / p).exists():
                missing.append(f"{doc.relative_to(root) if doc.is_relative_to(root) else doc}: {p}")
    return sorted(missing)


def sh(cmd: str, cwd: Path, env: dict) -> tuple[int, str]:
    p = subprocess.run(["bash", "-c", cmd], cwd=cwd, env=env, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


# --- 1. paths in this repository ----------------------------------------------

def check_repo_paths() -> None:
    t0 = time.time()
    missing = missing_paths(
        ROOT, [ROOT / "README.md", ROOT / "AGENTS.md"],
        ("server/", "web/", "crates/", "scripts/", ".github/", ".sqlx/", "cli/", "bench/"),
        app_only=True,
    )
    if missing:
        fail("paths", "\n".join(missing),
             "README.md or AGENTS.md mention paths that don't exist. Fix the docs or restore the files.")
    ok("paths mentioned in README.md and AGENTS.md exist", t0)


# --- 2-4. a new app ------------------------------------------------------------

def main() -> None:
    check_repo_paths()
    steps = list(MARKER.finditer(GUIDE.read_text()))
    if not steps:
        fail("guide", "no <!-- guide-test: ... --> markers found in GUIDE.md",
             "The walkthrough lost its markers; see the docstring of scripts/guide_test.py.")

    tmp = Path(tempfile.mkdtemp(prefix="northstack-guide-"))
    app = tmp / "app"
    env = dict(os.environ)
    env.update(
        DATABASE_URL="sqlite:data/dev.db",  # relative to the app: its own file
        CARGO_TARGET_DIR=str(ROOT / "target" / "guide-test"),
        CARGO_TERM_COLOR="never",
        NO_COLOR="1",
        # `northstack new` makes the first commit; CI machines have no git identity.
        GIT_AUTHOR_NAME="guide-test", GIT_AUTHOR_EMAIL="guide-test@localhost",
        GIT_COMMITTER_NAME="guide-test", GIT_COMMITTER_EMAIL="guide-test@localhost",
    )

    t0 = time.time()
    code, out = sh(f"cargo run -q -p northstack --bin northstack -- new {shlex.quote(str(app))} --local {shlex.quote(str(ROOT))}", ROOT, env)
    if code:
        fail("northstack new", out, "`northstack new` failed; see crates/northstack/src/cli.rs (new_app).", tmp)
    if (ROOT / "web/node_modules").is_dir():
        (app / "web/node_modules").symlink_to(ROOT / "web/node_modules")
    else:
        code, out = sh("cd web && bun install --frozen-lockfile", app, env)
        if code:
            fail("setup", out, "bun install failed in the new app.", tmp)
    code, out = sh("just check", app, env)
    if code or "all checks passed" not in out:
        fail("just check in a new app", out,
             "A freshly created app doesn't pass `just check`. Fix the template or the CLI.", tmp)
    code, out = sh("git status --porcelain", app, env)
    if code or out.strip():
        fail("clean tree after just check", out,
             "Checking a new app changed committed files (`just upgrade` then refuses to run). "
             "`northstack new` must commit them as the first build leaves them.", tmp)
    missing = missing_paths(
        app, [app / "AGENTS.md", app / "README.md", app / "docs/northstack.md"],
        ("server/", "web/", "cli/", "docs/", ".github/", ".sqlx/", ".cargo/"),
    )
    if missing:
        fail("paths in a new app", "\n".join(missing),
             "The app's docs mention paths a new app doesn't have. Fix GUIDE.md or the template.", tmp)
    framework = [p for p in ("crates", "bench", "scripts", ".github/workflows/framework.yml",
                             ".github/workflows/release.yml") if (app / p).exists()]
    if framework:
        fail("new app contents", f"copied: {framework}", "is_framework_file in cli.rs must exclude these.", tmp)
    ok("northstack new gives an app that passes just check", t0)

    t0 = time.time()
    code, out = sh("just remove-example", app, env)
    if code or "all checks passed" not in out:
        fail("just remove-example", out,
             "Removing the example no longer yields a clean app. Fix src/scripts/remove-example.sh.", tmp)
    ok("just remove-example gives a clean empty app", t0)

    for i, m in enumerate(steps, 1):
        action, arg, body = m.group(1), m.group(2), m.group(3)
        label = f"guide step {i}: {action} {arg or body.strip().splitlines()[0]}"
        t0 = time.time()
        if action == "run":
            code, out = sh(body, app, env)
        elif action == "migration":
            code, out = sh(f"just migration {arg}", app, env)
            if code == 0:
                (app / out.strip().splitlines()[-1]).write_text(body + "\n")
        elif action == "write":
            (app / arg).parent.mkdir(parents=True, exist_ok=True)
            (app / arg).write_text(body + "\n")
            code, out = 0, ""
        else:
            code, out = 1, f"unknown guide-test action '{action}'"
        if code:
            fail(label, out,
                 "The guide's walkthrough doesn't work as written. Update GUIDE.md (or fix the "
                 "framework) so this step succeeds, then run this test again.", tmp)
        ok(label[:60], t0)

    shutil.rmtree(tmp, ignore_errors=True)
    print(f"\nthe guide is true ({time.time() - START:.0f}s)")


if __name__ == "__main__":
    main()
