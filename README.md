<h1 align="center">northstack</h1>

<p align="center">
  <strong>Build web apps with AI agents.</strong><br>
  SQLite, Rust and React in one small binary.
</p>

<p align="center">
  <a href="#try-it">Try it</a> ·
  <a href="#the-guide">The guide</a> ·
  <a href="AGENTS.md">For agents</a>
</p>

---

northstack is an open-source, full-stack web framework: a **Rust** backend
(axum, sqlx), a **React** frontend with Next.js-style folder routing, and
**SQLite**, shipped as **one small binary**: no database server to run. It's
built for apps that AI
coding agents (Claude Code, Codex, Cursor) write and maintain: there's one
obvious way to do everything, and the compiler catches mistakes before you ever
see them.

## A feature in three files

**1. The table.** A SQL migration.

```sql
-- server/migrations/20261201_projects.sql
CREATE TABLE projects (
    id   UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
```

**2. The API.** Plain Rust functions. The `Project` type is generated from the
table above.

```rust
// server/src/features/projects/procedures.rs
pub async fn list(ctx: Ctx<AppState>, _input: ()) -> RpcResult<Vec<Project>> {
    Ok(db::list(&ctx.state.db).await?)
}

pub async fn create(ctx: Ctx<AppState>, input: NewProject) -> RpcResult<Project> {
    northstack::ensure!(!input.name.is_empty(), "name is required");
    Ok(db::insert(&ctx.state.db, &input.name).await?)
}
```

**3. The page.** The folder is the URL, like Next.js. The API is already typed
in React.

```tsx
// web/app/projects/page.tsx  ->  /projects
import { api } from "northstack";

export default function Projects() {
  const projects = api.projects.list.useQuery(null);
  const create = api.projects.create.useMutation();

  return (
    <>
      <h1>Projects</h1>
      <button onClick={() => create.mutate({ name: "Apollo" })}>Add Apollo</button>
      <ul>
        {projects.map((p) => <li key={p.id}>{p.name}</li>)}
      </ul>
    </>
  );
}
```

That's it: no REST routes, no fetch calls, no API types to write by hand. After
`create` succeeds, the list refreshes on its own.

## Mistakes don't get far

Types flow from the database to the screen, so a typo or a changed column is
caught by `just check` in a few seconds. Misspell a field in the example app:

```text
$ just check
ok   migrate    0s
ok   gen-db     0s
ok   compile    1s
ok   gen-ts     0s
FAIL typecheck  0s

app/page.tsx(95,25): error TS2561: Object literal may only specify known properties,
but 'titel' does not exist in type 'CreateNoteInput'. Did you mean to write 'title'?

how to fix: The frontend no longer matches the API. [...] Update the React code;
never edit generated files.
```

Rename a column, and the compiler lists every query and every screen that
still uses the old name. Nothing drifts silently.

## Compared to Next.js

The same app (same API, same queries, same indexes) built with northstack and
with Next.js 16 + Postgres 16. Each container gets 1 CPU and 512 MB, and both
start with 1,000 rows. With 50 users at once (median of 3 runs):

| | northstack | Next.js + Postgres | |
|---|---:|---:|---:|
| Memory at rest | **6 MB** | 134 MB | **21x less** |
| Memory under load | **12 MB** | 195 MB | **16x less** |
| Serve the page, requests/s | **39,836** | 3,210 | **12x faster** |
| Get one row, requests/s | **12,028** | 2,820 | **4.3x faster** |
| List 50 rows, requests/s | **4,868** | 1,649 | **3.0x faster** |
| Stats query (count + sum), requests/s | **8,235** | 3,120 | **2.6x faster** |
| Create a row, requests/s | **3,391** | 1,884 | **1.8x faster** |
| List 200 rows, requests/s | **1,133** | 746 | **1.5x faster** |
| Slowest 1% of requests, get one row | **26 ms** | 67 ms | **2.6x faster** |
| CPU used | **~0.95 core** | ~1.15 cores | **~17% less** |
| Start until ready | **0.11 s** | 0.28 s | **2.5x faster** |
| Docker images | **55 MB** | 1,012 MB | **18x smaller** |

At 1 user northstack is 1.2–3.7x faster on every read; Next.js is faster at
creating rows one at a time (northstack also runs a background job per row).
The method, every test at 1, 10 and 50 users, and how to rerun it:
[bench/README.md](bench/README.md). Measured November 2026 on a laptop.

## Why it's nice

- **Pages like Next.js.** A folder with a `page.tsx` is a URL. `[id]` is a
  parameter. `layout.tsx` wraps the pages below it.
- **One language per side.** Rust on the server, React in the browser, SQL for
  the data. Every npm package works in the browser.
- **Background jobs built in.** Send the email, resize the image, write the
  daily report: a job queue and a scheduler live in the same binary.
- **Small and cheap to run.** One binary and one database file. Single-digit MB
  of memory, 1.5–12x the throughput of a comparable Next.js + Postgres app
  ([see above](#compared-to-nextjs)). Memory is rarely what limits how many
  apps fit on one server.
- **Made for agents.** One command checks everything and prints only what's
  wrong. The guide below is tested in CI, so it always matches the code.

## FAQ

**Is northstack a Next.js alternative?** For apps behind a login (dashboards,
internal tools, SaaS), yes: it has Next.js-style pages (`page.tsx`, `layout.tsx`,
`[id]` folders) but a Rust backend, typed end to end, in one binary that uses a
few MB of memory.

**Does it do server-side rendering (SSR)?** No. Pages render in the browser,
which keeps it simple: no JavaScript runs on the server. For public pages that
need SEO, a static site or pre-rendering is the better fit.

**Which database does it use?** SQLite, in WAL mode: one file next to the
binary, no database server. Types for every table are generated from the
database, and every query is checked against it at compile time. It suits apps
that run on one machine (most do); it doesn't suit running many copies of the
app behind a load balancer.

**Can I use npm packages?** Yes, any package that runs in the browser. Server
code is Rust, so server-side work uses Rust crates.

**Do I need to know Rust?** It helps, but it's designed so an AI agent writes
most of the code: one pattern per task, a written guide, and checks that say
exactly what's wrong and how to fix it.

**How does it compare on cost?** An app uses about 6 MB of memory at rest and
needs no database server, against about 134 MB for a comparable Next.js +
Postgres app (see the [comparison](#compared-to-nextjs)). That matters when
you run many small apps on one server.

## Try it

You need Rust, Bun and `just`. On macOS:

```sh
brew install rustup bun just && rustup default stable
git clone https://github.com/elitan/northstack && cd northstack
just dev        # open http://localhost:5173
```

You'll see a small notes app. It's an **example** that shows every pattern
once; you replace it with your own features (`just remove-example` gives you an
empty app).

Then point your coding agent at the repo. It reads [AGENTS.md](AGENTS.md), which
sends it to [the guide](#the-guide) below.

---

# The guide

Everything an agent (or a person) needs to set up the project, build features,
test them and ship, in one place. Read it top to bottom once.

**The code under `server/` and `web/` is only an example** (a small "notes" app).
It exists to show every pattern once: a table, an enum, queries, procedures, a
background job, a cron entry, pages, URL state, mutations, and tests. Copy its
patterns, then replace it with your own features ([Starting your own
app](#starting-your-own-app)). The framework itself lives in `crates/northstack`.

## Contents

1. [The idea](#the-idea)
2. [How it fits together](#how-it-fits-together)
3. [Repository map](#repository-map)
4. [Setup](#setup)
5. [The working loop](#the-working-loop)
6. [Building a feature, end to end](#building-a-feature-end-to-end)
7. [Backend reference](#backend-reference)
8. [Frontend reference](#frontend-reference)
9. [Database](#database)
10. [Testing](#testing)
11. [Running in production](#running-in-production)
12. [Starting your own app](#starting-your-own-app)
13. [Rules](#rules)
14. [Not built yet](#not-built-yet)
15. [Changing the framework](#changing-the-framework)

## The idea

- **The database is the source of truth for types.** Row structs and enums are
  generated from the database schema into Rust, and from Rust into TypeScript.
  Add a column in a migration and the compiler points at every query and screen
  that must change.
- **One way to do each thing.** One command to check everything, one place for
  SQL, one way to call the server, one way to load data on a page.
- **Fail fast with short errors.** Every check prints only the failure and a
  "how to fix" line.
- **One binary.** The React frontend is embedded, migrations run at startup, and
  the same binary serves HTTP, runs background jobs and fires cron.

Stack: Rust (axum, sqlx with compile-time-checked SQL, SQLite) on the server;
React and TypeScript in the browser, built with Vite and Bun, with a small
Next.js-style folder router. There is no server-side rendering and no Node at runtime.

## How it fits together

Types flow one way. Every arrow is a generator, run by `just check`:

```
server/migrations/*.sql ──migrate──▶ SQLite ──gen-db──▶ server/src/db/schema.rs (row structs, enums)
                                                   ──gen-db──▶ server/sqlx.toml (column types for the query macros)
                                                              │
                                     your Rust: types.rs, db.rs, procedures.rs
                                                              │
                                     ──gen-ts──▶ web/northstack/ (TypeScript types, `api` client, router)
                                                              │
                                     your React: web/app/**/page.tsx
```

A request at runtime:

```
browser ── POST /rpc/notes.list {json} ──▶ axum ──▶ procedure(ctx, input) ──▶ sqlx ──▶ SQLite file
        ◀──────── 200 {json} or {code, message} with an HTTP status ◀──┘
```

Background work goes through a job table in the same database:

```
procedure: BEGIN; INSERT note; enqueue(job); COMMIT   ──▶ northstack_jobs row
worker (same binary): claim with one atomic UPDATE ──▶ handler(state, job) ──▶ done / retry / dead
cron (same binary): enqueue one job per schedule slot, exactly once
```

## Repository map

```
crates/northstack/        THE FRAMEWORK (generic; rarely changed)
  src/app.rs              App builder, CLI modes, HTTP router, config from env
  src/rpc.rs              Rpc registry: .query()/.mutation(), Ctx, POST /rpc/{name}
  src/error.rs            RpcError, ErrorCode, ensure! macro
  src/db.rs               SQLite settings, db::begin (write transactions), db::NOW
  src/jobs.rs             Job queue in the database: Job trait, Jobs registry, enqueue, workers
  src/cron.rs             Cron schedules that enqueue jobs, once per slot
  src/cpu.rs              Thread pool for CPU-heavy work (cpu::spawn)
  src/schema.rs           gen-db: SQLite schema -> Rust structs, enums and sqlx.toml
  src/codegen.rs          gen-ts: Rust procedures -> TypeScript client
  src/ts/*.ts             Static parts of the generated TypeScript client
  src/testing.rs          Test databases (testing::db) and an in-process HTTP client
  src/web.rs              Serves the embedded frontend (index.html fallback)
  src/bin/northstack.rs   Dev CLI: migrate, db-create, db-reset, migration, gen-db
  migrations/             The framework's own table (northstack_jobs)
  tests/                  Framework tests (jobs, database, schema generation, codegen)

server/                   YOUR BACKEND (the notes code here is an example)
  migrations/             SQL migrations, timestamped, append-only
  src/main.rs             Builds AppState and runs the App (rarely changed)
  src/lib.rs              build(): registers every feature
  src/state.rs            AppState: the db pool and anything else shared
  src/db/schema.rs        GENERATED from the database. Never edit.
  sqlx.toml               GENERATED column types for the sqlx macros. Never edit.
  src/features/<name>/    One folder per feature (see below)
  tests/<name>.rs         Integration tests through the real HTTP router

web/                      YOUR FRONTEND (the notes pages here are an example)
  app/                    Pages, like Next.js: folders are URLs (see "Frontend reference")
    layout.tsx            Wraps every page (nav, styles)
    page.tsx              /
    notes/[id]/page.tsx   /notes/:id
    not-found.tsx         404 page; error.tsx and loading.tsx: error and loading states
    components/           Shared components (any file that isn't page/layout is just code)
  e2e/                    Browser smoke tests (Playwright), run by `just e2e`
  northstack/             GENERATED: `api` client, types and the router. Never edit.
  index.html              Loads northstack/main.tsx

AGENTS.md                 Entry point for coding agents: the loop and the hard rules
scripts/check.sh          The one check command (see below)
scripts/dev.sh            Local environment: start, stop, status, reload, logs
scripts/new-feature.sh    Scaffolds a feature folder and registers it
scripts/remove-example.sh Deletes the example app (just remove-example)
scripts/readme_test.py    Runs the README walkthrough as written (just readme-test)
scripts/e2e.sh            Browser smoke tests against the real binary (just e2e)
bench/                    Benchmark against Next.js + Postgres (bench/README.md)
data/                     The dev database (data/dev.db), not committed
clippy.toml               Lint rules (e.g. use db::begin, not pool.begin)
.cargo/config.toml        SQLite build flags for concurrency (don't remove)
.sqlx/                    GENERATED offline query cache (lets builds run without a DB)
Dockerfile, justfile, .github/workflows/ci.yml
```

Every feature has the same five files:

| File | Contains |
|---|---|
| `types.rs` | API inputs and query-specific outputs, shared with TypeScript |
| `db.rs` | **All** SQL for the feature |
| `procedures.rs` | RPC handlers: validate, call `db`, enqueue jobs |
| `jobs.rs` | Background job types and handlers |
| `mod.rs` | `rpc()`, `jobs()` and `cron()`: the feature's registrations |

## Setup

Requirements: Rust (stable), Bun and `just`. No database server: SQLite is
compiled into the binary. On macOS:

```sh
brew install rustup bun just && rustup default stable
brew install sqlx-cli          # only needed by `just verify`
```

Then, from a fresh clone:

```sh
just dev                       # api on :3000 + Vite on :5173, in the background
```

`just dev` creates `.env` from `.env.example` if missing (the database is
`data/dev.db`, created on first use), installs frontend dependencies, runs
`just check`, then starts the api and Vite in the
background. It returns once both answer. Running it again is a no-op. Open
http://localhost:5173 (Vite proxies `/rpc` to the api).

## The working loop

Local development is built for agents: every command returns only when its
result is ready, and logs go to files instead of a terminal stream.

1. `just dev` once per session.
2. Make a change (migration, Rust, React).
3. `just check`. Fix what it reports and repeat until it passes.
4. Exercise the change: call the api, open a page, or run tests. When `just check`
   passes it has already reloaded the api and waited for `/readyz`, so whatever
   you call next runs the code you just wrote. Frontend changes hot-reload on
   their own.
5. `just verify` before you're done. A change is done only when it passes.
   Commit the generated files with your change.

`just check` runs these steps in order and stops at the first failure:

```text
$ just check
ok   migrate    0s     apply migrations to the dev database
ok   gen-db     0s     database -> server/src/db/schema.rs and server/sqlx.toml
ok   compile    4s     Rust, with every SQL query checked against the database
ok   gen-ts     0s     Rust -> web/northstack/
ok   typecheck  1s     React against the generated types
ok   reload     0s     restart the dev api (if running) and wait until ready

all checks passed in 6s
```

On failure it prints only the diagnostics and a "how to fix" line. For example,
after a migration adds `notes.pinned`:

```text
FAIL compile    0s

server/src/features/notes/db.rs:20:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
server/src/features/notes/db.rs:40:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
...
how to fix: Fix the errors above. 'error returned from database' means a query doesn't match
the schema; the current schema is in server/src/db/schema.rs. [...]
```

All commands:

| Command | What it does |
|---|---|
| `just dev` | Start the local environment (idempotent) |
| `just dev status` / `just dev stop` | Show / stop the api and Vite |
| `just dev reload` | Restart the api with the last build (`just check` does this) |
| `just logs` | Warnings and errors since the api last started (`just logs all`: everything) |
| `just check` | The fast loop above, 2–8 s |
| `just verify` | `check` + format, clippy (warnings are errors), all tests, `.sqlx` refresh, frontend build |
| `just ci` | Like `verify`, but fails if generated files are stale instead of writing them |
| `just migration <name>` | Create `server/migrations/<timestamp>_<name>.sql` |
| `just new-feature <name>` | Scaffold `server/src/features/<name>/` and register it |
| `just db-reset` | Delete, recreate and migrate the dev database file (destroys data) |
| `just remove-example` | Delete the example app and start from an empty one |
| `just e2e` | Browser smoke tests: build the binary, start it with its own database, run `web/e2e/` in headless Chromium (CI runs it) |
| `just readme-test` | Prove this README is true: run the feature walkthrough as written (CI runs it) |
| `just build` | Release binary with the frontend embedded: `target/release/app` |
| `just fmt` | Format Rust code |

Logs and process state live in `.dev/` (`api.log`, `web.log`).

## Building a feature, end to end

This walks through adding a feature called `projects`. Every step shows the
complete file, and names the file in the notes example that shows the same
pattern. CI runs this walkthrough exactly as written (`just readme-test`), so it
always matches the framework.

**1. Scaffold.**

<!-- readme-test: run -->
```sh
just new-feature projects
```

This creates `server/src/features/projects/` with the five files, registers the
feature in `server/src/lib.rs` and `server/src/features/mod.rs`, and adds a
`projects.ping` procedure and a test so it compiles and passes immediately. The
next steps replace those placeholder files.

**2. Schema.** `just migration create_projects` prints the new file's path.
Write the SQL there (example: `server/migrations/20261108000001_notes.sql`):

<!-- readme-test: migration create_projects -->
```sql
CREATE TABLE projects ( -- A project owned by a team.
    id         UUID      PRIMARY KEY NOT NULL,
    name       TEXT      NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    -- Archived projects are hidden from lists.
    status     TEXT      NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00'))
);
```

`just check` now generates `pub struct Project` and `pub enum ProjectStatus`
(from the `CHECK (status IN (...))`) in `server/src/db/schema.rs`, with docs
taken from the SQL comments, and records the column types in `server/sqlx.toml`. (It reports a
compile error until step 5 replaces the placeholder `ping` procedure; that's
expected.)

**3. Types**, `server/src/features/projects/types.rs` (example:
`server/src/features/notes/types.rs`). Re-export the generated row types; write
inputs and query-specific outputs by hand:

<!-- readme-test: write server/src/features/projects/types.rs -->
```rust
//! API types for projects. Row types come from the database (`crate::db`).

pub use crate::db::{Project, ProjectStatus};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    /// Defaults to `active`.
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<ProjectStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ListProjectsOutput {
    pub projects: Vec<Project>,
}
```

**4. SQL**, `server/src/features/projects/db.rs` (example:
`server/src/features/notes/db.rs`). Every query uses `sqlx::query!` or
`query_as!`, which are checked against the real schema at compile time:

<!-- readme-test: write server/src/features/projects/db.rs -->
```rust
//! All SQL for projects.

use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use super::types::*;

pub async fn insert<'e>(
    db: impl SqliteExecutor<'e>,
    name: &str,
    status: ProjectStatus,
) -> sqlx::Result<Project> {
    let id = Uuid::now_v7();
    sqlx::query_as!(
        Project,
        "INSERT INTO projects (id, name, status) VALUES ($1, $2, $3)
         RETURNING id, name, status, created_at",
        id,
        name,
        status,
    )
    .fetch_one(db)
    .await
}

pub async fn list(db: &SqlitePool) -> sqlx::Result<Vec<Project>> {
    sqlx::query_as!(
        Project,
        "SELECT id, name, status, created_at FROM projects ORDER BY created_at DESC"
    )
    .fetch_all(db)
    .await
}
```

**5. Procedures**, `server/src/features/projects/procedures.rs` (example:
`server/src/features/notes/procedures.rs`):

<!-- readme-test: write server/src/features/projects/procedures.rs -->
```rust
//! RPC handlers for projects.

use northstack::{Ctx, RpcResult};

use super::db;
use super::types::*;
use crate::AppState;

pub async fn list(ctx: Ctx<AppState>, _input: ()) -> RpcResult<ListProjectsOutput> {
    let projects = db::list(&ctx.state.db).await?;
    Ok(ListProjectsOutput { projects })
}

pub async fn create(ctx: Ctx<AppState>, input: CreateProjectInput) -> RpcResult<Project> {
    let name = input.name.trim();
    northstack::ensure!(!name.is_empty(), "name must not be empty");
    let status = input.status.unwrap_or(ProjectStatus::Active);
    Ok(db::insert(&ctx.state.db, name, status).await?)
}
```

**6. Register**, `server/src/features/projects/mod.rs` (example:
`server/src/features/notes/mod.rs`):

<!-- readme-test: write server/src/features/projects/mod.rs -->
```rust
//! Projects.

mod db;
mod jobs;
mod procedures;
pub mod types;

use crate::AppState;
use northstack::{Cron, Jobs, Rpc};

pub fn rpc() -> Rpc<AppState> {
    Rpc::new()
        .query("projects.list", procedures::list)
        .mutation("projects.create", procedures::create)
}

pub fn jobs() -> Jobs<AppState> {
    Jobs::new()
}

pub fn cron() -> Cron {
    Cron::new()
}
```

Run `just check`. `web/northstack/` now has `api.projects.list`,
`api.projects.create` and the `Project`, `ProjectStatus`, `CreateProjectInput`
and `ListProjectsOutput` TypeScript types.

**7. Page**, `web/app/projects/page.tsx` (example: `web/app/page.tsx`). The
folder is the URL: this page is `/projects`.

<!-- readme-test: write web/app/projects/page.tsx -->
```tsx
import { useState } from "react";
import { api } from "northstack";

export default function ProjectsPage() {
  const { projects } = api.projects.list.useQuery(null); // loaded before it renders
  const create = api.projects.create.useMutation(); // the list refetches after success
  const [name, setName] = useState("");

  return (
    <>
      <title>Projects</title>
      <h1>Projects</h1>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate({ name }, { onSuccess: () => setName("") });
        }}
      >
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name" />
        <button disabled={create.isPending}>Add</button>
        {create.error && <p className="error">{create.error.message}</p>}
      </form>
      <ul>
        {projects.map((p) => (
          <li key={p.id}>
            {p.name} · {p.status}
          </li>
        ))}
      </ul>
    </>
  );
}
```

**8. Tests**, `server/tests/projects.rs` (example: `server/tests/notes.rs`):

<!-- readme-test: write server/tests/projects.rs -->
```rust
//! Integration tests for projects, through the real HTTP router.

use std::time::Duration;

use server::features::projects::types::*;
use server::{AppState, MIGRATOR};
use northstack::ErrorCode;
use northstack::testing::{Client, TestDb};
use serde_json::json;

async fn setup() -> (TestDb, Client) {
    let db = northstack::testing::db(&MIGRATOR).await; // fresh, migrated, deleted after the test
    let router = server::build().router(AppState::new(db.pool()), db.pool(), Duration::from_secs(30));
    (db, Client::new(router))
}

#[tokio::test]
async fn create_and_list() {
    let (_db, client) = setup().await;
    let p: Project = client
        .call("projects.create", &json!({ "name": "  Apollo  " }))
        .await
        .unwrap();
    assert_eq!(p.name, "Apollo");
    assert_eq!(p.status, ProjectStatus::Active);

    let out: ListProjectsOutput = client.call("projects.list", &()).await.unwrap();
    assert_eq!(out.projects.len(), 1);
}

#[tokio::test]
async fn rejects_empty_name() {
    let (_db, client) = setup().await;
    let err = client
        .call::<_, Project>("projects.create", &json!({ "name": " " }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);
}
```

**9. Finish.**

<!-- readme-test: run -->
```sh
just verify
```

When it passes, commit everything, including the generated files.

## Backend reference

Everything below is exported from the `northstack` crate.

### Procedures

```rust
pub async fn name(ctx: Ctx<AppState>, input: In) -> RpcResult<Out>
```

- Registered with `Rpc::new().query("feature.name", f)` for reads and
  `.mutation("feature.name", f)` for writes. Names are dot-separated camelCase;
  the first segment is the namespace used by the frontend (`api.feature.name`).
- Served as `POST /rpc/{name}` with a JSON body. A procedure without input takes
  `()` (the client sends `null`).
- `In` and `Out` derive `Serialize, Deserialize, TS` with
  `#[serde(rename_all = "camelCase")]`. Optional input fields use
  `#[serde(default)] #[ts(optional)] pub field: Option<T>`. Ids are `Uuid`
  (a `string` in TypeScript). Invalid JSON is a `BAD_REQUEST`.
- `ctx.state` is your `AppState`. `ctx.header("name")` reads a request header.
  `ctx.ext::<T>()` / `ctx.require::<T>()` read a value placed in request
  extensions (`require` returns `UNAUTHORIZED` when it's missing; this is the
  hook for authentication).

### Errors

`RpcError` is `{ code, message }` with the matching HTTP status:

| Code | Status | Create with |
|---|---|---|
| `BAD_REQUEST` | 400 | `RpcError::bad_request(msg)` or `northstack::ensure!(cond, "msg {}", x)` |
| `UNAUTHORIZED` | 401 | `RpcError::unauthorized(msg)` |
| `FORBIDDEN` | 403 | `RpcError::forbidden(msg)` |
| `NOT_FOUND` | 404 | `RpcError::not_found(msg)` |
| `CONFLICT` | 409 | `RpcError::conflict(msg)` |
| `INTERNAL` | 500 | `RpcError::internal(cause)`: the cause is logged, the client sees only "internal server error" |

`?` converts errors for you: sqlx `RowNotFound` becomes `NOT_FOUND`, a unique
violation becomes `CONFLICT`, anything else becomes `INTERNAL`. An
`anyhow::Error` that wraps an `RpcError` keeps its code.

### AppState

`server/src/state.rs` holds what every procedure and job needs (the pool, and later
config or API clients). It must be cheap to `Clone`. It is built in
`server/src/main.rs` from the connected pool.

### Background jobs

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SendReport { pub project_id: Uuid }

impl northstack::Job for SendReport {
    const KIND: &'static str = "projects.send_report"; // stable: stored in the database
    const MAX_ATTEMPTS: i32 = 5;                         // optional, default 5
    const TIMEOUT: Duration = Duration::from_secs(300);  // optional, default 300 s
}

pub async fn send_report(state: AppState, job: SendReport) -> anyhow::Result<()> { ... }
```

- Register with `Jobs::new().register(jobs::send_report)` in the feature's
  `jobs()`.
- Enqueue **in the same transaction** as the write that causes it, so both
  commit or neither does: `northstack::enqueue(&mut *tx, &SendReport { .. }).await?`.
  `enqueue_with(db, &job, EnqueueOptions { run_at, run_key })` schedules for
  later, or deduplicates by `run_key`.
- **Handlers must be idempotent.** A job can run more than once (a crash after
  the work but before it is marked done).
- Failures retry with exponential backoff up to `MAX_ATTEMPTS`, then the job is
  marked `dead`. A panic or a timeout counts as a failure and doesn't affect
  other jobs. Finished jobs are deleted after 7 days.
- Workers claim with one atomic `UPDATE ... RETURNING`, so a job never runs
  twice at once, even when two processes share the database during a deploy.
  An enqueue wakes this process's idle workers right away; they also poll
  every second (that picks up jobs enqueued by `app serve` in another process).
- CPU-heavy work (parsing, big computations) goes through
  `northstack::cpu::spawn(move || work()).await?`, never directly in async code.
  It runs on a separate thread pool so it never blocks requests.

### Cron

```rust
pub fn cron() -> Cron {
    // sec min hour day-of-month month day-of-week, in UTC
    Cron::new().add("projects.nightly_report", "0 0 2 * * *", |at| NightlyReport { day: at.date_naive() })
}
```

Cron only enqueues a job; the job does the work. Each schedule slot is enqueued
exactly once, even when two processes are up during a deploy. If the app is
down (or asleep) at a scheduled time, that run is skipped.

### Other HTTP routes

For endpoints that aren't RPC (webhooks, file downloads), add a plain axum
router in `server/src/lib.rs`: `.routes(axum::Router::new().route("/webhooks/x", post(handler)))`.
Built-in routes: `POST /rpc/{name}`, `GET /healthz` (liveness), `GET /readyz`
(database reachable). Everything else serves the embedded frontend, with
`index.html` as the fallback for client-side routes.

## Frontend reference

A client-rendered React app. Everything comes from one import:
`import { api, navigate, type Note } from "northstack"` (generated into
`web/northstack/`).

**Pages: folders are URLs**, like Next.js's app router:

| File | URL / role |
|---|---|
| `web/app/page.tsx` | `/` |
| `web/app/about/page.tsx` | `/about` |
| `web/app/notes/[id]/page.tsx` | `/notes/:id`, with `params.id` |
| `web/app/(admin)/users/page.tsx` | `/users`: a `(group)` folder doesn't appear in the URL |
| `web/app/layout.tsx` | Wraps every page below its folder; layouts nest |
| `web/app/not-found.tsx` | Unknown URLs, `notFound()`, and `NOT_FOUND` errors from the API |
| `web/app/error.tsx` | Any other error while a page renders (gets `{ error, retry }`) |
| `web/app/loading.tsx` | Shown while a page's data loads |

A page is a default export that gets `{ params, searchParams }`. Other files in
`web/app/` (components, helpers) are ordinary modules. Set the browser title
with `<title>` anywhere in the page.

```tsx
// web/app/notes/[id]/page.tsx
import { api, type PageProps } from "northstack";

export default function NotePage({ params }: PageProps<{ id: string }>) {
  const note = api.notes.get.useQuery({ id: params.id }); // never undefined
  return (
    <>
      <title>{note.title}</title>
      <h1>{note.title}</h1>
    </>
  );
}
```

**Data:**

| Call | What it does |
|---|---|
| `api.ns.query.useQuery(input)` | Loads data; the page shows `loading.tsx` until it's there, then gets it (never undefined). Errors go to `error.tsx`, `NOT_FOUND` to `not-found.tsx`. |
| `api.ns.query.useQuery(input, { refetchInterval })` | The same, polling (ms, or a function of the data; `false` stops) |
| `api.ns.mutation.useMutation({ onSuccess })` | Writes: `.mutate(input)`, `.isPending`, `.error`, `.data`. After success, `onSuccess` runs, then **every query refetches**, so nothing needs listing. |
| `api.ns.proc.call(input)` | Plain async call, outside React (mutations also refetch) |
| `refresh()` | Refetch everything (e.g. after a change the page didn't make) |
| `isRpcError(error, "CONFLICT")` | Check an error's code |

To leave a page after a mutation (e.g. after a delete), navigate in
`useMutation({ onSuccess })`: it runs before the refetch, so the deleted item
isn't fetched again.

**Navigation:** plain `<a href="/notes/1">` links navigate without a full page
load. In code: `navigate("/notes/1")`, or `navigate("?q=milk", { replace: true })`
to change only the search. Hooks: `usePathname()`, `useSearchParams()`,
`useParams()`. `notFound()` shows the not-found page.

**State in the URL.** Filters, search, tabs and pagination go in search params
(`searchParams` prop, `navigate("?...")`), so they survive reloads and can be
shared (example: `web/app/page.tsx`). Use component state only for form inputs
and UI toggles.

**Don't** call `fetch` directly, use another router, or copy server data into
`useState` (except to seed a form).

## Database

One SQLite file (`DATABASE_URL`, `sqlite:data/dev.db` in development), opened
in WAL mode: reads run in parallel and never block, writes take turns (each
waits up to 5 s for the previous one). That's thousands of short writes per
second; keep transactions short.

**Declared types.** SQLite stores few types, so a column's declared type says
what it holds. `gen-db` maps it to Rust (and on to TypeScript):

| Declare | Rust | TypeScript | Notes |
|---|---|---|---|
| `INTEGER` | `i64` | `number` | `INTEGER PRIMARY KEY` is an auto-increment id |
| `REAL` | `f64` | `number` | |
| `TEXT` | `String` | `string` | |
| `TEXT CHECK (col IN ('a', 'b'))` | an enum | `"a" \| "b"` | Named struct + column: `notes.priority` → `NotePriority` |
| `BOOLEAN` | `bool` | `boolean` | Stored as 0/1; `DEFAULT false` works |
| `UUID` | `Uuid` | `string` | Generate in Rust: `Uuid::now_v7()` (sorts by time) |
| `TIMESTAMP` | `DateTime<Utc>` | `string` | Default must be `(strftime('%Y-%m-%dT%H:%M:%f+00:00'))` |
| `DATE` | `NaiveDate` | `string` | `2026-11-15` |
| `JSON` | `serde_json::Value` | `JsonValue` | |
| `BLOB` | `Vec<u8>` | `number[]` | |

Anything else (`NUMERIC`, no type at all, ...) is reported by `just check`.
There are no exact decimals: store money as `INTEGER` cents. Write `NOT NULL`
on every column that can't be null, including a `UUID PRIMARY KEY` (SQLite
allows NULL there otherwise). Nullable columns are `Option<T>`.

**Times** are UTC text like `2026-11-15T10:00:00.123+00:00`. Set them in SQL
with `strftime('%Y-%m-%dT%H:%M:%f+00:00')` (e.g. `updated_at =
strftime('%Y-%m-%dT%H:%M:%f+00:00')`) or bind a `DateTime<Utc>` from Rust.
Both forms compare and sort correctly as text. `date(col)`, `date($1, '+1
day')` and the other SQLite date functions work on them. Don't use
`CURRENT_TIMESTAMP` or `datetime('now')`: their format sorts wrong against
times from Rust (`just check` rejects them as a `TIMESTAMP` default).

**Generated types.** `server/src/db/schema.rs` has one struct per table
(`notes` → `Note`) and one enum per `CHECK (col IN (...))` column, with serde
camelCase and `TS`. Use them in queries and return them from procedures.
`server/sqlx.toml` tells the query macros the same column types, so `SELECT id,
priority, created_at ...` decodes into `Uuid`, `NotePriority` and
`DateTime<Utc>` without annotations.

**Conventions in SQL comments** (inside `CREATE TABLE`):

```sql
CREATE TABLE people ( -- Someone with an account. @name(Person)
    id            UUID PRIMARY KEY NOT NULL,
    -- Login email, unique.
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL, -- Argon2 hash. @private
    plan          TEXT NOT NULL CHECK (plan IN ('free', 'pro')) -- @enum(Plan)
);
```

- A comment on the `CREATE TABLE` line documents the table; a comment above a
  column or after it on the same line documents the column. Both become
  documentation in Rust and TypeScript.
- `@private` keeps a column out of JSON and TypeScript. Use it for password
  hashes and anything else the browser must not see.
- `@name(Person)` on the table sets the struct name (`people` → `Person`), for
  when the automatic singular is wrong. `@enum(Plan)` names an enum.
- `@skip` on the table skips it. Virtual tables (FTS5 search) are skipped
  automatically.
- `ALTER TABLE ... ADD COLUMN` can't carry a comment: SQLite copies the column
  text into the table definition, and a trailing `--` comment breaks it. To
  document a column added later, rebuild the table (below).

**Writing queries:**

- All SQL lives in the feature's `db.rs`, via `sqlx::query!` / `query_as!`
  with bind parameters (`$1`, `$2`, ...). Never build SQL with `format!`.
- Optional filters: `WHERE ($1 IS NULL OR name LIKE '%' || $1 || '%')`. `LIKE`
  ignores case for ASCII letters only.
- Computed columns (`count(*)`, `sum`, `CASE`, subqueries) have no table type;
  name it in the alias: `count(*) AS "total!: i64"` (`!` = never null),
  `max(day) AS "last: chrono::NaiveDate"`.
- Parameters are not type-checked by SQLite: bind the Rust type that matches
  the column (`Uuid` for a `UUID` column, not its string).
- Enums sort as text. To sort by meaning, use `ORDER BY CASE status WHEN 'high'
  THEN 2 ... END`.
- **Every list needs an index that matches its `ORDER BY`**, or SQLite copies
  and sorts the whole table on each request (with 1,000 rows, a 200-row list
  went from 0.6 ms to 0.04 ms with the index). An index can hold the same
  expression: `CREATE INDEX notes_list ON notes ((CASE priority WHEN 'high'
  THEN 2 ... END) DESC, created_at DESC, id DESC)` (example:
  `server/migrations/20261108000001_notes.sql`). Check with `EXPLAIN QUERY
  PLAN`: "USE TEMP B-TREE FOR ORDER BY" means a sort.
- End an `ORDER BY` on a unique column (`created_at DESC, id DESC`). Rows
  written in the same millisecond have equal times, and without a tiebreaker
  their order is arbitrary.
- Take `impl SqliteExecutor<'e>` instead of `&SqlitePool` when the query must
  also work inside a transaction (`&mut *tx`).
- Start transactions with `northstack::db::begin(&ctx.state.db)`, never
  `pool.begin()` (clippy rejects it). It takes the write lock up front, so two
  transactions that read and then write wait for each other instead of failing
  with "database is locked".
- When a table gains a column, every `query_as!(Table, ...)` must select it;
  the compiler lists each query. Joins and aggregates return hand-written
  structs in `types.rs`.

**Migrations:**

- Append-only: never edit a migration that has been applied anywhere. In
  development, `just db-reset` starts over.
- Backward compatible (expand, then contract): during a deploy, the old binary
  keeps running for a moment against the new schema.
- `ALTER TABLE` can add a column (`ADD COLUMN`, with a `DEFAULT` if it's `NOT
  NULL`), rename a table or column, and drop a column. To change a column's
  type, constraint or enum values, rebuild the table in one migration, exactly
  like this:

  ```sql
  -- no-transaction
  PRAGMA foreign_keys = OFF;
  BEGIN;
  CREATE TABLE notes_new ( ... the new definition ... );
  INSERT INTO notes_new (id, title, ...) SELECT id, title, ... FROM notes;
  DROP TABLE notes;
  ALTER TABLE notes_new RENAME TO notes;
  -- recreate the table's indexes here
  COMMIT;
  PRAGMA foreign_keys = ON;
  ```

  The first three lines matter. With foreign keys on, `DROP TABLE notes`
  deletes every row in other tables that reference notes (`ON DELETE
  CASCADE`), or fails. `PRAGMA foreign_keys` can only change outside a
  transaction, so the migration opts out of the automatic one
  (`-- no-transaction`, the first line) and opens its own. References from
  other tables follow the rename and keep working.

- Use `CHECK (col IN (...))` for fixed sets of values. The enum flows into Rust
  and TypeScript, so adding a value (a table rebuild) makes the compiler point
  at every place that must handle it.

## Testing

- **Integration tests** go in `server/tests/<feature>.rs`.
  `northstack::testing::db(&MIGRATOR)` gives each test its own fresh, migrated
  database file (deleted when the test ends), so tests run in parallel. Tests
  call procedures through the real HTTP router with `northstack::testing::Client`:

  ```rust
  #[tokio::test]
  async fn create_project() {
      let db = northstack::testing::db(&server::MIGRATOR).await;
      let app = server::build();
      let client = Client::new(app.router(AppState::new(db.pool()), db.pool(), Duration::from_secs(30)));
      let p: Project = client.call("projects.create", &json!({ "name": "Apollo" })).await.unwrap();
      assert_eq!(p.status, ProjectStatus::Active);
  }
  ```

- Error cases: `client.call(...)` returns `Err(RpcError)`; assert on `.code`.
- Jobs: `app.jobs_ref().run_pending(&db, state).await` runs everything queued.
- Cron: `app.cron_ref().fire(&db, "projects.nightly_report", at).await`
  enqueues one slot.
- `just verify` runs every test. The framework's own tests are in
  `crates/northstack/tests`.
- **Browser smoke tests** go in `web/e2e/*.spec.ts` (Playwright). `just e2e`
  builds the frontend into the binary, starts it on :3100 with its own database
  (`target/e2e/app.db`, reset each run) and runs them in headless Chromium; CI runs
  it too. Keep them few: one test per important user flow, using labels and
  roles (`page.getByLabel("Title")`), not CSS classes. `app.spec.ts` checks any
  app (loads without console errors, unknown URLs 404); `notes.spec.ts` walks
  through the example. `just e2e notes` runs only matching files.

## Running in production

Build: `just build` (binary at `target/release/app`) or
`docker build -t app .`. The Docker build uses the committed `.sqlx/` cache, so
it needs no database. The image is distroless and runs as non-root.

**The database is a file**, `/data/app.db` in the image. Mount a persistent
volume at `/data` on the machine's local disk (`docker run -v app-data:/data
app`, or a volume in your platform, e.g. frost). Never put it on a network file
system (NFS, SMB, Azure Files): SQLite's locking doesn't work there and the
file can be corrupted. Migrations run at startup. Zero-downtime deploys are
fine: the old and new process share the file for a moment, and SQLite
coordinates them.

One binary, four modes:

| Command | Runs |
|---|---|
| `app` | HTTP + job workers + cron (default) |
| `app serve` | HTTP only |
| `app worker` | Job workers and cron only |
| `app migrate` | Apply migrations and exit |

Configuration (environment):

| Variable | Default | |
|---|---|---|
| `DATABASE_URL` | `sqlite:data/app.db` | The database file (`sqlite:/data/app.db` in the image); created if missing |
| `PORT` / `HOST` | `3000` / `0.0.0.0` | |
| `DB_MAX_CONNECTIONS` | `8` | Pool size (readers run in parallel; writers take turns) |
| `WORKER_CONCURRENCY` | `8` | Concurrent jobs per process |
| `MIGRATE_ON_START` | `true` | |
| `REQUEST_TIMEOUT_SECS` | `30` | |
| `CPU_THREADS` | cores − 1 | Size of the CPU pool |
| `LOG_FORMAT` | `json` | `pretty` for development |
| `RUST_LOG` | `info` | |

**Scaling.** One app = one process on one machine, with all state (including
the job queue) in its database file. Scale up (more CPU, faster disk), not out:
don't run several copies behind a load balancer, each would get its own file.
`app serve` and `app worker` exist to run HTTP and jobs as separate processes
on the same machine and volume.

**Backups.** Copy the file safely while the app runs with SQLite's `VACUUM
INTO` (a consistent snapshot) or stream it to object storage with
[Litestream](https://litestream.io). A built-in backup command isn't there yet
([Not built yet](#not-built-yet)).

**Measured** against an equivalent Next.js 16 + Kysely + Postgres 16 app, each
container limited to 1 CPU and 512 MB: about 6 MB of memory at rest and 12 MB
under load (Next.js + Postgres: 134 and 195 MB), and 1.5–12x the requests per
second at 50 concurrent users. Full results and the harness:
[bench/README.md](bench/README.md).

**SQLite build flags.** `.cargo/config.toml` builds the bundled SQLite without
`SQLITE_ENABLE_MEMORY_MANAGEMENT` (which puts every connection's page cache
behind one global lock) and with `SQLITE_DEFAULT_MEMSTATUS=0`. With the default
build, concurrent queries take turns: listing rows at 10 users was about 5x
slower. A test in `crates/northstack/tests/db.rs` checks the flags.

**CI** (`.github/workflows/ci.yml`) runs `just ci` against a fresh database:
every generated file must be up to date, and format, lint, tests and the
frontend build must pass. Then it runs `just readme-test`: starting from
`just remove-example`, it follows [Building a
feature](#building-a-feature-end-to-end) exactly as written and runs `just
verify` on the result, so the guide can't drift from the framework.

## Starting your own app

The notes feature is an example. To start from an empty app:

```sh
just remove-example
```

It deletes the notes feature, its tests, migrations, pages and component, gives
you a minimal home page, resets the dev database, and runs `just check`, which
rewrites every generated file for the empty app. It refuses to run with
uncommitted changes (`--force` skips that), so `git` can always undo it.

Then add your features with `just new-feature <name>` and follow [Building a
feature](#building-a-feature-end-to-end). Rename the project in `README.md` and
`Cargo.toml` if you like; keep `crates/northstack` as it is.

In a project that's already deployed, don't delete applied migrations: remove
the code, and add a migration that drops the example tables.

## Rules

- Never edit generated files: `server/src/db/schema.rs`, `server/sqlx.toml`,
  `web/northstack/`, `.sqlx/`. Regenerate with `just check`.
- Never redefine a table's row type by hand; use the generated one.
- All SQL in `db.rs`, with bind parameters.
- Enqueue jobs in the same transaction as the write that causes them, and make
  handlers idempotent.
- Don't hold a transaction open across slow I/O (HTTP calls, big CPU work).
- Don't keep state in memory that must survive a restart. Use the database.
- Start write transactions with `northstack::db::begin`, never `pool.begin()`.
- Don't add a dependency when the standard library or an existing dependency
  covers it.
- A change is done when `just verify` passes and the generated files are
  committed.

## Not built yet

- **Authentication.** Planned: a native Rust solution with a typed
  `ctx.user()` in procedures and a protected layout route in React. Until then,
  `ctx.require::<T>()` is the hook, but there is no built-in middleware to
  populate it.
- **Backups.** Planned: a built-in snapshot (`VACUUM INTO`) uploaded to
  S3-compatible storage on a schedule. Until then, use Litestream or copy a
  `VACUUM INTO` snapshot yourself.
- Admin UI, file storage, realtime updates, rate limiting, security headers,
  audit logging, OpenTelemetry.
- Generated insert/update types per table (deliberately: inputs are written by
  hand so clients can't set columns they shouldn't).
- Exact decimals (store cents as `INTEGER`), arrays (use `JSON` or a child
  table). Joins and aggregates use hand-written types.

## Changing the framework

`crates/northstack/src` is small (about 2.5k lines) and fully tested
(`crates/northstack/tests`). Change it when a pattern repeats across features,
keep its public API small, and add a test for every behavior change. Apps
should only need what `lib.rs` exports: `App`, `Rpc`, `Ctx`, `RpcResult`,
`RpcError`, `ErrorCode`, `ensure!`, `Job`, `Jobs`, `enqueue`, `enqueue_with`,
`EnqueueOptions`, `Cron`, `db`, `cpu`, `testing`, and the re-exported crates (`sqlx`,
`axum`, `serde`, `chrono`, `uuid`, `ts_rs`, `tokio`, `tracing`).
