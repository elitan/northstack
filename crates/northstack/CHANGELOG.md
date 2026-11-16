# Changelog

Every release of northstack. `just upgrade` prints the sections for all the
versions it moves an app past, so each one says exactly what an app must
change, under **Upgrade**, as steps an agent can follow. A release with nothing
to change says so.

## 0.1.0

First release: the Rust server (RPC, background jobs, cron, SQLite), the React
frontend with folder routing and generated types, and the `northstack` CLI
(`new`, `check`, `dev`, `e2e`, `new-feature`, `upgrade`, database commands).

**Upgrade:** nothing to do.
