<h1 align="center">northstack</h1>

<p align="center">
  <strong>Web apps your AI agents can build, change and run without getting lost.</strong><br>
  Postgres, Rust and React in one small binary, with types that flow from the database to the screen.
</p>

<p align="center">
  <a href="#get-started">Get started</a> ·
  <a href="#the-guide">The guide</a> ·
  <a href="AGENTS.md">For agents</a>
</p>

---

Agents are good at writing code and bad at guessing. Most stacks make them
guess: which of five ways to fetch data, whether the API still matches the
database, whether the server they just restarted is running the new code.

northstack removes the guessing. There's one way to do each thing, the compiler
knows the whole app, and every command tells you exactly what's wrong and how
to fix it.

### Change the database, and the compiler shows you the rest

```sql
ALTER TABLE notes ADD COLUMN pinned BOOLEAN NOT NULL DEFAULT false;
```

```text
$ just check
ok   migrate    0s
ok   gen-db     0s
FAIL compile    0s

app/src/features/notes/db.rs:18:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
app/src/features/notes/db.rs:38:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
app/src/features/notes/db.rs:57:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
app/src/features/notes/db.rs:82:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`

how to fix: Fix the errors above. [...] the current schema is in app/src/db/schema.rs.
```

Fix the query, run it again, and the new field is in the TypeScript types the
frontend uses. Add a value to a Postgres enum and React stops type-checking at
the exact component that doesn't handle it yet. Nothing drifts silently.

### What you get

|  |  |
|---|---|
| **Types end to end** | Postgres → Rust → TypeScript, all generated. SQL is checked against the real database at compile time. |
| **Typed RPC** | Write `async fn` in Rust; call `api.projects.list.useSuspenseQuery()` in React. No REST routes, no schemas to keep in sync. |
| **Background jobs and cron** | A Postgres job queue in the same binary: transactional enqueue, retries, safe across any number of replicas. |
| **One binary** | Frontend embedded, migrations run at startup. Deploy one container, or run it as `serve` and `worker` to scale them separately. |
| **Small and fast** | About 2 MiB of memory idle and 8 MiB under load. Several times the throughput of an equivalent Next.js app on the same 0.25 vCPU ([measured](#running-in-production)). |
| **Made for the agent loop** | `just check` regenerates everything, compiles, type-checks and reloads the dev server in a few seconds, then prints only what failed. |

### What it looks like

```rust
// Rust: a procedure
pub async fn create(ctx: Ctx<AppState>, input: CreateProjectInput) -> RpcResult<Project> {
    northstack::ensure!(!input.name.trim().is_empty(), "name must not be empty");
    Ok(db::insert(&ctx.state.db, input.name.trim()).await?)
}
```

```tsx
// React: the same procedure, typed from the Rust above
const create = api.projects.create.useMutation({ invalidates: ["projects"] });
create.mutate({ name: "Apollo" });   // create.mutate({ nme: "Apollo" }) won't compile
```

### Get started

You need Rust, Bun, `just` and Docker (for the local Postgres). On macOS:

```sh
brew install rustup bun just && rustup default stable
git clone https://github.com/elitan/northstack && cd northstack
just dev        # Postgres, api and frontend, ready at http://localhost:5173
```

Then point your agent at the repo. It reads [AGENTS.md](AGENTS.md), which sends
it to [the guide](#the-guide) below.

The code in `app/` and `web/` is **an example** (a small notes app) that shows
every pattern once. You replace it with your own features; the framework in
`crates/northstack` stays.

---

# The guide

Everything an agent (or a person) needs to set up the project, build features,
test them and ship, in one place. Read it top to bottom once.

**The code under `app/` and `web/` is only an example** (a small "notes" app).
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

- **Postgres is the source of truth for types.** Row structs and enums are
  generated from the database schema into Rust, and from Rust into TypeScript.
  Add a column in a migration and the compiler points at every query and screen
  that must change.
- **One way to do each thing.** One command to check everything, one place for
  SQL, one way to call the server, one way to load data on a page.
- **Fail fast with short errors.** Every check prints only the failure and a
  "how to fix" line.
- **One binary.** The React frontend is embedded, migrations run at startup, and
  the same binary serves HTTP, runs background jobs and fires cron.

Stack: Rust (axum, sqlx with compile-time-checked SQL, Postgres) on the server;
React, TypeScript, TanStack Router and TanStack Query in the browser, built with
Vite and Bun. There is no server-side rendering and no Node at runtime.

## How it fits together

Types flow one way. Every arrow is a generator, run by `just check`:

```
app/migrations/*.sql ──migrate──▶ Postgres ──gen-db──▶ app/src/db/schema.rs (row structs, enums)
                                                              │
                                     your Rust: types.rs, db.rs, procedures.rs
                                                              │
                                     ──gen-ts──▶ web/src/rpc/ (TypeScript types + `api` client)
                                                              │
                                     your React: web/src/routes/*.tsx
```

A request at runtime:

```
browser ── POST /rpc/notes.list {json} ──▶ axum ──▶ procedure(ctx, input) ──▶ sqlx ──▶ Postgres
        ◀──────── 200 {json} or {code, message} with an HTTP status ◀──┘
```

Background work goes through a job table in the same Postgres:

```
procedure: BEGIN; INSERT note; enqueue(job); COMMIT   ──▶ northstack_jobs row
worker (same binary, any replica): claim with SKIP LOCKED ──▶ handler(state, job) ──▶ done / retry / dead
cron (same binary, any replica): enqueue one job per schedule slot, exactly once
```

## Repository map

```
crates/northstack/        THE FRAMEWORK (generic; rarely changed)
  src/app.rs              App builder, CLI modes, HTTP router, config from env
  src/rpc.rs              Rpc registry: .query()/.mutation(), Ctx, POST /rpc/{name}
  src/error.rs            RpcError, ErrorCode, ensure! macro
  src/jobs.rs             Postgres job queue: Job trait, Jobs registry, enqueue, workers
  src/cron.rs             Cron schedules that enqueue jobs, safe across replicas
  src/cpu.rs              Thread pool for CPU-heavy work (cpu::spawn)
  src/schema.rs           gen-db: Postgres schema -> Rust structs and enums
  src/codegen.rs          gen-ts: Rust procedures -> TypeScript client
  src/ts/*.ts             Static parts of the generated TypeScript client
  src/testing.rs          In-process HTTP client for tests
  src/web.rs              Serves the embedded frontend (index.html fallback)
  src/bin/northstack.rs   Dev CLI: migrate, db-create, db-reset, migration, gen-db
  migrations/             The framework's own table (northstack_jobs)
  tests/                  Framework tests (jobs, schema generation, codegen)

app/                      YOUR BACKEND (the notes code here is an example)
  migrations/             SQL migrations, timestamped, append-only
  src/main.rs             Builds AppState and runs the App (rarely changed)
  src/lib.rs              build(): registers every feature
  src/state.rs            AppState: the db pool and anything else shared
  src/db/schema.rs        GENERATED from Postgres. Never edit.
  src/features/<name>/    One folder per feature (see below)
  tests/<name>.rs         Integration tests through the real HTTP router

web/                      YOUR FRONTEND (the notes pages here are an example)
  src/main.tsx            QueryClient + router setup (rarely changed)
  src/routes/             Pages. One file = one URL. __root.tsx is the layout.
  src/components/         Shared components. states.tsx = loading/error/not-found.
  src/rpc/                GENERATED client (`api`) and types. Never edit.
  src/routeTree.gen.ts    GENERATED route tree. Never edit.

AGENTS.md                 Entry point for coding agents: the loop and the hard rules
scripts/check.sh          The one check command (see below)
scripts/dev.sh            Local environment: start, stop, status, reload, logs
scripts/new-feature.sh    Scaffolds a feature folder and registers it
compose.yaml              Local Postgres for development
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

Requirements: Rust (stable), Bun, `just`, and Docker (or your own Postgres).
On macOS:

```sh
brew install rustup bun just && rustup default stable
brew install sqlx-cli          # only needed by `just verify`
```

Then, from a fresh clone:

```sh
just dev                       # Postgres + api on :3000 + Vite on :5173, in the background
```

`just dev` creates `.env` from `.env.example` if missing, starts Postgres with
Docker (`compose.yaml`) unless `DATABASE_URL` is already reachable, installs
frontend dependencies, runs `just check`, then starts the api and Vite in the
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
ok   gen-db     0s     Postgres -> app/src/db/schema.rs
ok   compile    4s     Rust, with every SQL query checked against Postgres
ok   gen-ts     0s     Rust -> web/src/rpc/
ok   typecheck  1s     React against the generated types and routes
ok   reload     0s     restart the dev api (if running) and wait until ready

all checks passed in 6s
```

On failure it prints only the diagnostics and a "how to fix" line. For example,
after a migration adds `notes.pinned`:

```text
FAIL compile    0s

app/src/features/notes/db.rs:18:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
app/src/features/notes/db.rs:38:9: error[E0063]: missing field `pinned` in initializer of `db::schema::Note`
...
how to fix: Fix the errors above. 'error returned from database' means a query doesn't match
the schema; the current schema is in app/src/db/schema.rs. [...]
```

All commands:

| Command | What it does |
|---|---|
| `just dev` | Start the local environment (idempotent) |
| `just dev status` / `just dev stop` | Show / stop the api and Vite (Postgres keeps running) |
| `just dev reload` | Restart the api with the last build (`just check` does this) |
| `just logs` | Warnings and errors since the api last started (`just logs all`: everything) |
| `just check` | The fast loop above, 2–8 s |
| `just verify` | `check` + format, clippy (warnings are errors), all tests, `.sqlx` refresh, frontend build |
| `just ci` | Like `verify`, but fails if generated files are stale instead of writing them |
| `just migration <name>` | Create `app/migrations/<timestamp>_<name>.sql` |
| `just new-feature <name>` | Scaffold `app/src/features/<name>/` and register it |
| `just db-reset` | Drop, recreate and migrate the dev database (destroys data) |
| `just build` | Release binary with the frontend embedded: `target/release/app` |
| `just fmt` | Format Rust code |

Logs and process state live in `.dev/` (`api.log`, `web.log`).

## Building a feature, end to end

This walks through adding a feature called `projects`. Every step names the
file in the notes example that shows the same pattern.

**1. Scaffold.** `just new-feature projects` creates
`app/src/features/projects/` with the five files, registers it in
`app/src/lib.rs` and `app/src/features/mod.rs`, and adds a `projects.ping`
procedure so it compiles immediately.

**2. Schema.** `just migration create_projects`, then write SQL in the printed
file (example: `app/migrations/20261108000001_notes.sql`):

```sql
CREATE TYPE project_status AS ENUM ('active', 'archived');

CREATE TABLE projects (
    id         UUID           PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT           NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    status     project_status NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ    NOT NULL DEFAULT now()
);
COMMENT ON TABLE projects IS 'A project owned by a team.';
```

Run `just check`. `app/src/db/schema.rs` now has `pub struct Project` and
`pub enum ProjectStatus`, with docs taken from the SQL comments.

**3. Types** in `types.rs` (example: `features/notes/types.rs`). Re-export the
generated row types and write inputs and query-specific outputs by hand:

```rust
pub use crate::db::{Project, ProjectStatus};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
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

**4. SQL** in `db.rs` (example: `features/notes/db.rs`), with
`use sqlx::{PgExecutor, PgPool};` and `use super::types::*;`. Every query uses
`sqlx::query!` or `query_as!`, which are checked against the real schema at
compile time:

```rust
pub async fn insert<'e>(db: impl PgExecutor<'e>, name: &str, status: ProjectStatus) -> sqlx::Result<Project> {
    sqlx::query_as!(
        Project,
        r#"INSERT INTO projects (name, status) VALUES ($1, $2)
           RETURNING id, name, status AS "status: ProjectStatus", created_at"#,
        name,
        status as ProjectStatus
    )
    .fetch_one(db)
    .await
}

pub async fn list(db: &PgPool) -> sqlx::Result<Vec<Project>> {
    sqlx::query_as!(
        Project,
        r#"SELECT id, name, status AS "status: ProjectStatus", created_at
           FROM projects ORDER BY created_at DESC"#
    )
    .fetch_all(db)
    .await
}
```

**5. Procedures** in `procedures.rs` (example: `features/notes/procedures.rs`).
The scaffold already imports `Ctx`, `RpcResult`, the types and `AppState`; add
`use super::db;`:

```rust
pub async fn list(ctx: Ctx<AppState>, _input: ()) -> RpcResult<ListProjectsOutput> {
    Ok(ListProjectsOutput { projects: db::list(&ctx.state.db).await? })
}

pub async fn create(ctx: Ctx<AppState>, input: CreateProjectInput) -> RpcResult<Project> {
    let name = input.name.trim();
    northstack::ensure!(!name.is_empty(), "name must not be empty");
    let status = input.status.unwrap_or(ProjectStatus::Active);
    Ok(db::insert(&ctx.state.db, name, status).await?)
}
```

**6. Register** in `mod.rs` (example: `features/notes/mod.rs`):

```rust
pub fn rpc() -> Rpc<AppState> {
    Rpc::new()
        .query("projects.list", procedures::list)
        .mutation("projects.create", procedures::create)
}
```

Run `just check`. `web/src/rpc/` now has `api.projects.create` and the
`Project`, `ProjectStatus` and `CreateProjectInput` TypeScript types.

**7. Page** in `web/src/routes/projects.tsx` (example: `web/src/routes/index.tsx`):

```tsx
import { createFileRoute } from "@tanstack/react-router";
import { api } from "../rpc";

export const Route = createFileRoute("/projects")({
  loader: ({ context: { queryClient } }) => api.projects.list.ensureData(queryClient, null),
  component: Projects,
});

function Projects() {
  const { projects } = api.projects.list.useSuspenseQuery(null);
  const create = api.projects.create.useMutation({ invalidates: ["projects"] });
  // render projects; call create.mutate({ name }) from a form
}
```

**8. Tests** in `app/tests/projects.rs` (example: `app/tests/notes.rs`), then
`just verify`.

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

`app/src/state.rs` holds what every procedure and job needs (the pool, and later
config or API clients). It must be cheap to `Clone`. It is built in
`app/src/main.rs` from the connected pool.

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
- Workers claim with `FOR UPDATE SKIP LOCKED` and wake instantly via
  `LISTEN/NOTIFY`, so any number of replicas can run them.
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
exactly once across all replicas (no leader election needed).

### Other HTTP routes

For endpoints that aren't RPC (webhooks, file downloads), add a plain axum
router in `app/src/lib.rs`: `.routes(axum::Router::new().route("/webhooks/x", post(handler)))`.
Built-in routes: `POST /rpc/{name}`, `GET /healthz` (liveness), `GET /readyz`
(database reachable). Everything else serves the embedded frontend, with
`index.html` as the fallback for client-side routes.

## Frontend reference

A client-rendered single-page app. All server data goes through the generated
`api` object (`import { api } from "../rpc"`).

**Pages.** A file in `web/src/routes/` is a page: `index.tsx` → `/`,
`about.tsx` → `/about`, `notes.$noteId.tsx` → `/notes/:noteId`. `__root.tsx`
is the layout. Each exports `const Route = createFileRoute("/path")({...})`.
Links and params are type-checked: `<Link to="/notes/$noteId" params={{ noteId }} />`.

**Load in the loader, read with suspense.** Every page that shows server data
prefetches in its `loader`, so it renders with data and never shows a spinner
inside the page:

```tsx
export const Route = createFileRoute("/notes/$noteId")({
  loader: ({ context: { queryClient }, params }) =>
    api.notes.get.ensureData(queryClient, { id: params.noteId }),
  component: NotePage,
});
function NotePage() {
  const { noteId } = Route.useParams();
  const note = api.notes.get.useSuspenseQuery({ id: noteId }); // never undefined
}
```

Use `api.x.y.useQuery(input)` (not suspense) only for optional or on-demand data
inside a page.

**The generated `api`:**

| Call | Use |
|---|---|
| `api.ns.query.ensureData(queryClient, input)` | In loaders |
| `api.ns.query.useSuspenseQuery(input, options?)` | In page components |
| `api.ns.query.useQuery(input, options?)` | Optional / on-demand data |
| `api.ns.query.queryOptions(input)` | When TanStack Query needs options directly |
| `api.ns.mutation.useMutation({ invalidates, onSuccess, ... })` | Writes |
| `api.ns.proc.call(input)` | Plain async call, outside React |
| `isRpcError(error, "CONFLICT")` | Check an error's code |

**Mutations declare what they invalidate:** `invalidates: ["notes"]` refetches
every `notes.*` query; `invalidates: ["notes.list"]` refetches one. After a
delete, invalidate the list rather than the deleted item's own query.

**State in the URL.** Filters, search, tabs and pagination go in search params
via `validateSearch` and `loaderDeps` (example: `routes/index.tsx`), so they
survive reloads and can be shared. Use component state only for form inputs and
UI toggles.

**Errors.** Loaders turn `NOT_FOUND` into `throw notFound()` (example:
`routes/notes.$noteId.tsx`). Anything else reaches the error view in
`components/states.tsx`, which has a retry button. Show mutation errors inline
with `mutation.error.message`. Queries retry only `INTERNAL` errors.

**Don't** call `fetch` directly, build query keys by hand, or copy server data
into `useState`.

## Database

**Generated types.** `app/src/db/schema.rs` has one struct per table
(`notes` → `Note`) and one enum per Postgres enum (`note_priority` →
`NotePriority`), with serde camelCase and `TS`. Use them in queries and return
them from procedures. Nullable columns are `Option<T>`. Supported column types:
bool, int2/4/8, float4/8, numeric (`BigDecimal`, a string in TS), text/varchar,
uuid, timestamptz, timestamp, date, time, json/jsonb, bytea, enums, and arrays
of these. `just check` names any unsupported column.

**Conventions in SQL comments:**

- `COMMENT ON TABLE/COLUMN ... IS '...'` becomes documentation in Rust and TS.
- `@private` in a column comment keeps it out of JSON and TypeScript. Use it for
  password hashes and anything else the browser must not see.
- `@name(Person)` in a table comment sets the struct name (`people` → `Person`).
  Use it when the automatic singular is wrong.
- `@skip` in a table comment skips the table.

**Writing queries:**

- All SQL lives in the feature's `db.rs`, via `sqlx::query!` / `query_as!`
  with bind parameters. Never build SQL with `format!`.
- Enum columns need a type hint: `SELECT status AS "status: ProjectStatus"`, and
  `status as ProjectStatus` for parameters.
- Optional filters: `WHERE ($1::text IS NULL OR name ILIKE '%' || $1 || '%')`.
- Take `impl PgExecutor<'e>` instead of `&PgPool` when the query must also work
  inside a transaction (`&mut *tx`).
- When a table gains a column, every `query_as!(Table, ...)` must select it;
  the compiler lists each query. Joins and aggregates return hand-written
  structs in `types.rs`.

**Migrations:**

- Append-only: never edit a migration that has been applied anywhere. In
  development, `just db-reset` starts over.
- Backward compatible (expand, then contract): during a rolling deploy, old and
  new binaries run against the same schema.
- Prefer Postgres enums and `CHECK` constraints for fixed sets of values. An
  enum flows into Rust and TypeScript, so adding a value makes the compiler point
  at every place that must handle it.

## Testing

- **Integration tests** go in `app/tests/<feature>.rs`. `#[sqlx::test]` gives
  each test a fresh database; tests call procedures through the real HTTP
  router with `northstack::testing::Client`:

  ```rust
  #[sqlx::test(migrations = false)]
  async fn create_project(pool: PgPool) {
      northstack::migrate(&pool, &app::MIGRATOR).await.unwrap();
      let app = app::build();
      let client = Client::new(app.router(AppState::new(pool.clone()), pool.clone(), Duration::from_secs(30)));
      let p: Project = client.call("projects.create", &json!({ "name": "Apollo" })).await.unwrap();
      assert_eq!(p.status, ProjectStatus::Active);
  }
  ```

- Error cases: `client.call(...)` returns `Err(RpcError)`; assert on `.code`.
- Jobs: `app.jobs_ref().run_pending(&pool, state).await` runs everything queued.
- Cron: `app.cron_ref().fire(&pool, "projects.nightly_report", at).await`
  enqueues one slot.
- `just verify` runs every test. The framework's own tests are in
  `crates/northstack/tests`.

## Running in production

Build: `just build` (binary at `target/release/app`) or
`docker build -t app .`. The Docker build uses the committed `.sqlx/` cache, so
it needs no database. The image is distroless and runs as non-root.

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
| `DATABASE_URL` | required | Postgres connection string |
| `PORT` / `HOST` | `3000` / `0.0.0.0` | |
| `DB_MAX_CONNECTIONS` | `10` | Pool size per process |
| `WORKER_CONCURRENCY` | `8` | Concurrent jobs per process |
| `JOBS_LISTEN` | `true` | Set `false` behind PgBouncer in transaction mode |
| `MIGRATE_ON_START` | `true` | |
| `REQUEST_TIMEOUT_SECS` | `30` | |
| `CPU_THREADS` | cores − 1 | Size of the CPU pool |
| `LOG_FORMAT` | `json` | `pretty` for development |
| `RUST_LOG` | `info` | |

**Scaling.** The binary is stateless; all state, including the job queue, is in
Postgres. Run as many replicas as you like. To scale HTTP and jobs separately,
deploy the same image twice, as `app serve` and `app worker`.

**Measured** on Azure Container Apps (0.25 vCPU / 0.5 GiB, Postgres in the same
region), against an equivalent Next.js 16 + Kysely app with the same endpoints
and SQL: about 2 MiB of memory idle and 8 MiB under load (Next.js: 40 and 102
MiB), and 3–4× the requests per second on endpoints where the app, not the
database, does the work (for example 293 vs 80 req/s listing 50 rows at 50
concurrent users). When the database is the bottleneck, both are equal.

**CI** (`.github/workflows/ci.yml`) runs `just ci` against a fresh Postgres:
every generated file must be up to date, and format, lint, tests and the
frontend build must pass.

## Starting your own app

The notes feature is an example. To replace it with your own app:

1. Delete the example backend: `app/src/features/notes/`, `app/tests/notes.rs`,
   and the `pub mod notes;` line in `app/src/features/mod.rs`. In
   `app/src/lib.rs`, remove the three `features::notes::...` lines from `build()`
   (keep `.migrations`, `.frontend` and the `// northstack:features` marker).
2. Delete the example migrations in `app/migrations/` (only in a new project
   that has never been deployed; otherwise add a migration that drops the
   tables).
3. Delete the example pages: `web/src/routes/notes.$noteId.tsx` and
   `web/src/components/priority.tsx`. Rewrite `web/src/routes/index.tsx` and
   `about.tsx`, and the links in `__root.tsx`.
4. `just db-reset`, then `just check`. Generated files are rewritten for the
   empty app.
5. Add your features with `just new-feature <name>` and follow
   [Building a feature](#building-a-feature-end-to-end).
6. Rename the project in `README.md` and `Cargo.toml` if you like. Keep
   `crates/northstack` as it is.

## Rules

- Never edit generated files: `app/src/db/schema.rs`, `web/src/rpc/`,
  `web/src/routeTree.gen.ts`, `.sqlx/`. Regenerate with `just check`.
- Never redefine a table's row type by hand; use the generated one.
- All SQL in `db.rs`, with bind parameters.
- Enqueue jobs in the same transaction as the write that causes them, and make
  handlers idempotent.
- Don't hold a transaction open across slow I/O (HTTP calls, big CPU work).
- Don't keep state in memory that must survive a restart or be shared across
  replicas. Use Postgres.
- Don't add a dependency when the standard library or an existing dependency
  covers it.
- A change is done when `just verify` passes and the generated files are
  committed.

## Not built yet

- **Authentication.** Planned: a native Rust solution with a typed
  `ctx.user()` in procedures and a protected layout route in React. Until then,
  `ctx.require::<T>()` is the hook, but there is no built-in middleware to
  populate it.
- Admin UI, file storage, realtime updates, rate limiting, security headers,
  audit logging, OpenTelemetry.
- Generated insert/update types per table (deliberately: inputs are written by
  hand so clients can't set columns they shouldn't).
- Postgres types without generated mappings: inet, geometric types, ranges.
  Joins and aggregates use hand-written types.

## Changing the framework

`crates/northstack/src` is small (about 2.5k lines) and fully tested
(`crates/northstack/tests`). Change it when a pattern repeats across features,
keep its public API small, and add a test for every behavior change. Apps
should only need what `lib.rs` exports: `App`, `Rpc`, `Ctx`, `RpcResult`,
`RpcError`, `ErrorCode`, `ensure!`, `Job`, `Jobs`, `enqueue`, `enqueue_with`,
`EnqueueOptions`, `Cron`, `cpu`, `testing`, and the re-exported crates (`sqlx`,
`axum`, `serde`, `chrono`, `uuid`, `ts_rs`, `tokio`, `tracing`).
