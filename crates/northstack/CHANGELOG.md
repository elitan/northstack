# Changelog

Every release of northstack. `just upgrade` prints the sections for all the
versions it moves an app past, so each one says exactly what an app must
change, under **Upgrade**, as steps an agent can follow.

Every change that affects apps adds a line under **Unreleased** in the same
commit (CI checks this). A release turns **Unreleased** into the new
version's section (`python3 scripts/release.py`).

## Unreleased

First release: the Rust server (RPC, background jobs, cron, SQLite), the React
frontend with folder routing and generated types, and the `northstack` CLI
(`new`, `check`, `dev`, `e2e`, `new-feature`, `upgrade`, database commands).

**Upgrade:** nothing to do.
