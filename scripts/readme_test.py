#!/usr/bin/env python3
"""`just readme-test`: prove the README is true.

1. paths:   every repo path the README or AGENTS.md mentions exists.
2. example: in a temporary copy of the repo, `just remove-example` produces a
            clean empty app.
3. guide:   the "Building a feature" walkthrough, executed exactly as written.
            Steps are fenced code blocks preceded by a hidden marker:

              <!-- readme-test: run -->                 run the block in bash
              <!-- readme-test: migration <name> -->    `just migration <name>`, write the block to it
              <!-- readme-test: write <path> -->        write the block to <path>

The copy uses its own database (`<your db>_readme_test`) and shares `target/`
across runs (target/readme-test), so it doesn't touch your data and recompiles
incrementally without invalidating your main build.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parent.parent
README = ROOT / "README.md"
DOCS = [README, ROOT / "AGENTS.md"]
MARKER = re.compile(
    r"<!-- readme-test: (\w+)(?: (\S+))? -->\s*\n```[\w-]*\n(.*?)\n```", re.DOTALL
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
        print(f"\nthe temporary copy is kept for inspection: {keep}")
    sys.exit(1)


# --- 1. paths -----------------------------------------------------------------

def check_paths() -> None:
    t0 = time.time()
    missing = []
    prefixes = ("app/", "web/", "crates/", "scripts/", ".github/", ".sqlx/")
    for doc in DOCS:
        text = doc.read_text()
        # Paths created by the walkthrough itself, or at build/run time.
        created = set(m.group(2) for m in MARKER.finditer(text) if m.group(1) == "write")
        candidates = set(re.findall(r"`([^`\s]+)`", text))
        candidates |= set(re.findall(r"\]\(([^)#\s]+)\)", text))
        for c in candidates:
            p = c.rstrip(".,:;")
            if not (p.startswith(prefixes) or p.endswith(".md")) or "://" in p:
                continue
            if any(ch in p for ch in "<>*{}") or p in created or "/projects" in p:
                continue
            if p.startswith(("web/dist", ".dev")) or p.endswith(".gen.ts.bak"):
                continue
            if not (ROOT / p).exists():
                missing.append(f"{doc.name}: {p}")
    if missing:
        fail("paths", "\n".join(sorted(missing)),
             "The docs mention paths that don't exist. Fix the docs or restore the files.")
    ok("paths mentioned in README.md and AGENTS.md exist", t0)


# --- 2 + 3. walkthrough in a temporary copy -----------------------------------

def database_url() -> str:
    url = os.environ.get("DATABASE_URL")
    if not url and (ROOT / ".env").exists():
        for line in (ROOT / ".env").read_text().splitlines():
            if line.startswith("DATABASE_URL="):
                url = line.split("=", 1)[1].strip()
    if not url:
        fail("setup", "DATABASE_URL is not set", "Run 'just dev' (creates .env and starts Postgres) or set DATABASE_URL.")
    parts = urlsplit(url)
    return urlunsplit(parts._replace(path=parts.path.rstrip("/") + "_readme_test"))


def copy_repo(dest: Path) -> None:
    files = subprocess.run(
        ["git", "ls-files", "-z", "-co", "--exclude-standard"],
        cwd=ROOT, capture_output=True, check=True,
    ).stdout.decode().split("\0")
    for f in filter(None, files):
        src = ROOT / f
        if src.is_file():
            (dest / f).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dest / f)
    # build.rs creates web/dist, but only when its build script reruns.
    (dest / "web/dist").mkdir(parents=True, exist_ok=True)
    if (ROOT / "web/node_modules").is_dir():
        (dest / "web/node_modules").symlink_to(ROOT / "web/node_modules")


def sh(cmd: str, cwd: Path, env: dict) -> tuple[int, str]:
    p = subprocess.run(["bash", "-c", cmd], cwd=cwd, env=env, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def walkthrough() -> None:
    steps = list(MARKER.finditer(README.read_text()))
    if not steps:
        fail("guide", "no <!-- readme-test: ... --> markers found in README.md",
             "The walkthrough lost its markers; see the docstring of scripts/readme_test.py.")

    tmp = Path(tempfile.mkdtemp(prefix="northstack-readme-"))
    env = dict(os.environ)
    env.update(
        DATABASE_URL=database_url(),
        CARGO_TARGET_DIR=str(ROOT / "target" / "readme-test"),
        CARGO_TERM_COLOR="never",
        NO_COLOR="1",
    )
    copy_repo(tmp)
    if not (tmp / "web/node_modules").exists():
        code, out = sh("cd web && bun install --frozen-lockfile", tmp, env)
        if code:
            fail("setup", out, "bun install failed in the temporary copy.", tmp)

    t0 = time.time()
    code, out = sh("./scripts/remove-example.sh --force", tmp, env)
    if code or "all checks passed" not in out:
        fail("just remove-example", out,
             "Removing the example no longer yields a clean app. Fix scripts/remove-example.sh.", tmp)
    ok("just remove-example gives a clean empty app", t0)

    for i, m in enumerate(steps, 1):
        action, arg, body = m.group(1), m.group(2), m.group(3)
        label = f"guide step {i}: {action} {arg or body.strip().splitlines()[0]}"
        t0 = time.time()
        if action == "run":
            code, out = sh(body, tmp, env)
        elif action == "migration":
            code, out = sh(f"just migration {arg}", tmp, env)
            if code == 0:
                (tmp / out.strip().splitlines()[-1]).write_text(body + "\n")
        elif action == "write":
            (tmp / arg).parent.mkdir(parents=True, exist_ok=True)
            (tmp / arg).write_text(body + "\n")
            code, out = 0, ""
        else:
            code, out = 1, f"unknown readme-test action '{action}'"
        if code:
            fail(label, out,
                 "The README walkthrough doesn't work as written. Update the README (or fix the "
                 "framework) so this step succeeds, then run 'just readme-test' again.", tmp)
        ok(label[:60], t0)

    shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    check_paths()
    walkthrough()
    print(f"\nthe README is true ({time.time() - START:.0f}s)")
