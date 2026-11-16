# AGENTS.md

This repo is a northstack app: Rust (axum + sqlx/SQLite) on the server, React +
TypeScript in the browser with Next.js-style folder routing (`web/app/`), typed
RPC in between, shipped as one binary.

**Read [README.md](README.md) from "The guide" onwards before your first change.**
It is the complete reference: architecture, every command, how to build a
feature end to end, the backend and frontend APIs, database conventions,
testing, deployment, and what isn't built yet. This file only repeats the parts
you must never miss.

The code in `server/` and `web/` is an example ("notes"). Copy its patterns; don't
treat its features as requirements.

## The loop

```sh
just dev       # once per session: api :3000, Vite :5173, in the background
just check     # after every change: regenerate, compile, typecheck, reload the api
just verify    # before you're done: check + fmt + clippy + tests + build
```

- `just check` returns only when the api is running your new code. Call it or
  open http://localhost:5173 right after.
- Read failures literally. Each one ends with a "how to fix" line.
- `just logs` shows runtime warnings and errors since the api last started.
- A change is done when `just verify` passes and generated files are committed.
  If you changed pages, also run `just e2e` (browser smoke tests; CI runs them).

## Never

- Edit generated files: `server/src/db/schema.rs`, `server/sqlx.toml`,
  `web/northstack/`, `.sqlx/`. They come from `just check`.
- Edit a migration that has been applied. Add a new one (`just migration <name>`).
- Write SQL outside a feature's `db.rs`, or build SQL with `format!`.
- Hand-write a table's row type. Use the generated struct from `crate::db`.
- Call `fetch` in React or add another router. Use `api` and the `web/app/` folders.
- Enqueue a job outside the transaction of the write that causes it, or write a
  job handler that isn't safe to run twice.
- Keep state in memory that must survive restarts. Use the database (SQLite).
- Start a transaction with `pool.begin()`. Use `northstack::db::begin(&pool)`.
- Use `CURRENT_TIMESTAMP`/`datetime('now')`. Times are
  `strftime('%Y-%m-%dT%H:%M:%f+00:00')` in SQL or a bound `DateTime<Utc>`.

## Where things go

| You want to | Do this |
|---|---|
| Add a feature | `just new-feature <name>`, then README → "Building a feature" |
| Change the schema | `just migration <name>`, write SQL, `just check` (column types: README → "Database") |
| Add an endpoint | Function in `procedures.rs`, register in the feature's `mod.rs` |
| Add a page | `web/app/<path>/page.tsx` (folders are URLs); data with `api.x.y.useQuery(input)` |
| Run work later | A `Job` in `jobs.rs`; schedule it with `Cron` if it's periodic |
| Test it | `server/tests/<feature>.rs` with `northstack::testing::{db, Client}` |
| Test a user flow in the browser | `web/e2e/<flow>.spec.ts` (Playwright); run `just e2e` |
| Start from an empty app | `just remove-example` |
| Change the framework or README | Add a test for every behavior change; run `just readme-test` |
