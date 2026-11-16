# AGENTS.md

This repo is a northstack app: Rust (axum + sqlx/SQLite) on the server, React +
TypeScript in the browser with Next.js-style folder routing (`web/app/`), typed
RPC in between, shipped as one binary.

**Read [docs/northstack.md](docs/northstack.md) before your first change.** It
is the framework guide for the exact northstack version this app uses: the
architecture, every command, how to build a feature end to end, the backend and
frontend APIs, database conventions, testing, deployment, and what isn't built
yet. This file only repeats the parts you must never miss.

If `server/src/features/notes` exists, it is the example app. Copy its patterns;
don't treat its features as requirements.

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

## Shipping a change

The human prompts; you do the whole flow, through a pull request:

1. Branch from an up-to-date `main`.
2. Make the change with tests; `just verify` passes (and `just e2e` if pages changed).
3. Commit (generated files included), push, `gh pr create`.
4. Wait for CI (`gh pr checks <number> --watch`). If it fails, read the log
   (`gh run view <run-id> --log-failed`), fix, push, repeat. Never merge red.
5. Merge when CI is green and the human asked for it to land:
   `gh pr merge <number> --squash --delete-branch`.
6. Deploy only commits whose CI passed, as described in
   [docs/northstack.md](docs/northstack.md) ("Running in production").

Don't push to `main` directly or force-push it. Upgrade northstack in its own
PR (`just upgrade`, then follow the notes it prints).

## Never

- Edit generated files: `server/src/db/schema.rs`, `server/sqlx.toml`,
  `web/northstack/`, `.sqlx/`. They come from `just check`. `docs/northstack.md`
  and `cli/` belong to the framework; `just upgrade` replaces them.
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
| Add a feature | `just new-feature <name>`, then the guide → "Building a feature" |
| Change the schema | `just migration <name>`, write SQL, `just check` (column types: the guide → "Database") |
| Add an endpoint | Function in `procedures.rs`, register in the feature's `mod.rs` |
| Add a page | `web/app/<path>/page.tsx` (folders are URLs); data with `api.x.y.useQuery(input)` |
| Run work later | A `Job` in `jobs.rs`; schedule it with `Cron` if it's periodic |
| Test it | `server/tests/<feature>.rs` with `northstack::testing::{db, Client}` |
| Test a user flow in the browser | `web/e2e/<flow>.spec.ts` (Playwright); run `just e2e` |
| Start from an empty app | `just remove-example` |
| Update northstack | `just upgrade` (commit first), then follow the upgrade notes it prints |
