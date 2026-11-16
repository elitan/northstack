#!/usr/bin/env python3
"""`python3 scripts/release_notes.py v0.2.0`: check that a release tag matches
the crate, and print that version's changelog section (the release notes).

Fails if the tag isn't `v<version>` for the version in
crates/northstack/Cargo.toml, or if crates/northstack/CHANGELOG.md has no
`## <version>` section. The release workflow runs it before publishing.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent / "crates/northstack"


def notes(changelog: str, version: str) -> str | None:
    """The body of the `## <version>` section, or None."""
    for section in changelog.split("\n## ")[1:]:
        heading, _, body = section.partition("\n")
        if heading.split()[0] == version:
            return body.strip()
    return None


def main(tag: str) -> int:
    # The first `version = "..."` is the package's ([package] comes first).
    version = re.search(r'^version = "([^"]+)"', (CRATE / "Cargo.toml").read_text(), re.M).group(1)
    if not re.fullmatch(r"v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?", tag):
        print(f"error: tag {tag!r} isn't v<major>.<minor>.<patch>", file=sys.stderr)
        return 1
    if tag != f"v{version}":
        print(f"error: tag {tag} doesn't match the crate version {version} "
              "(crates/northstack/Cargo.toml)", file=sys.stderr)
        return 1
    body = notes((CRATE / "CHANGELOG.md").read_text(), version)
    if not body:
        print(f"error: crates/northstack/CHANGELOG.md has no '## {version}' section "
              "(what changed, and the **Upgrade** steps)", file=sys.stderr)
        return 1
    print(body)
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
