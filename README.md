# northstack

A small framework for single-binary web apps built and maintained by AI agents.

- **Postgres is the source of truth for types:** row structs and enums are
  generated from the schema into Rust, then from Rust into TypeScript. Add a
  column in a migration and the compiler points at every query and screen that
  must change.
- **Rust backend:** axum + sqlx with compile-time-checked SQL against Postgres.
- **Typed RPC to React, tRPC-style:** register plain async functions in Rust and
  call them as `api.notes.list.useSuspenseQuery(...)`. Types, client, and
  TanStack Query helpers are generated, and CI fails on drift.
- **Next.js-like frontend without SSR:** file-based routing with TanStack Router,
  data loaded in route loaders, typed URL params and search params, per-route
  code splitting, and shared not-found, error and loading states.
- **Background jobs in the same binary:** a Postgres queue that is safe across
  replicas (`SKIP LOCKED`), transactional enqueue, retries with backoff,
  leases, timeouts, panic isolation, and `LISTEN/NOTIFY` wake-up.
- **Cron that is safe across replicas:** each schedule slot is enqueued exactly
  once, with no leader election.
- **One binary:** the frontend is embedded, migrations run at startup, and the
  same binary runs as `app`, `app serve`, `app worker`, or `app migrate`.

## Quick start

Requirements: Rust, Bun, and Docker (or your own Postgres). On macOS:

```sh
brew install rustup bun just && rustup default stable
just dev                      # Postgres + api on :3000 + Vite on :5173, in the background
just check                    # the loop: regenerate, compile, typecheck, reload the api
```

Local development is built for agents, so every command finishes only when
its result is ready, and logs go to files rather than a terminal stream:

- `just dev` creates `.env` if missing, starts Postgres with Docker
  (`compose.yaml`) unless `DATABASE_URL` is reachable, installs frontend
  dependencies, runs `just check`, then starts the api and Vite in the
  background. It returns once both answer. Running it again is a no-op.
- `just check` reloads the running api after a passing check and waits for
  `/readyz`, so the next request or page load runs the code you just wrote.
  If the new binary crashes on startup, the check fails with its log lines.
- Vite hot-reloads frontend changes in place.
- `just logs` prints warnings and errors since the api last started (`just logs all`
  for everything). `just dev status` and `just dev stop` do what they say.
  State and logs live in `.dev/`.

`sqlx-cli` (`brew install sqlx-cli`) is only needed for `just verify`.

## One command

```text
$ just check
ok   migrate    0s     apply migrations to the dev database
ok   gen-db     0s     Postgres -> app/src/db/schema.rs
ok   compile    4s     Rust, with every SQL query checked against Postgres
ok   gen-ts     0s     Rust -> web/src/rpc/
ok   typecheck  1s     React against the generated types and routes

all checks passed in 6s
```

On failure it prints only the diagnostics and a "how to fix" line. For example,
after a migration adds `notes.pinned`:

```text
FAIL compile    1s

app/src/features/notes/db.rs:18:9: error[E0063]: missing field `pinned` in initializer of `Note`
app/src/features/notes/db.rs:38:9: error[E0063]: missing field `pinned` in initializer of `Note`
```

`just verify` adds formatting, lint, tests, the offline query cache and the
frontend build. `just ci` does the same but fails on stale generated files
instead of rewriting them.

Release build with the frontend embedded:

```sh
just build                    # -> target/release/app
DATABASE_URL=... ./target/release/app
```

Docker: `docker build -t app .` (uses the committed `.sqlx/` cache, no DB needed).

## What a feature looks like

```rust
// types.rs: shared with TypeScript
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteInput { pub title: String }

// procedures.rs
pub async fn create(ctx: Ctx<AppState>, input: CreateNoteInput) -> RpcResult<Note> {
    northstack::ensure!(!input.title.trim().is_empty(), "title must not be empty");
    let mut tx = ctx.state.db.begin().await?;
    let note = db::insert(&mut *tx, &input.title).await?;
    northstack::enqueue(&mut *tx, &AnalyzeNote { note_id: note.id }).await?; // same transaction
    tx.commit().await?;
    Ok(note)
}

// mod.rs
Rpc::new().mutation("notes.create", procedures::create)
```

```tsx
// web/src/routes/index.tsx: a page. Typed URL params, data loaded before render.
export const Route = createFileRoute("/")({
  validateSearch: (s): { q?: string } => ({ q: typeof s.q === "string" ? s.q : undefined }),
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, deps }) =>
    api.notes.list.ensureData(queryClient, { search: deps.q }),
  component: Notes,
});

function Notes() {
  const { q } = Route.useSearch();
  const { notes } = api.notes.list.useSuspenseQuery({ search: q });  // typed from Rust
  const create = api.notes.create.useMutation({ invalidates: ["notes"] });
  // create.mutate({ titel: "x" }) -> TypeScript error
}
```

## Configuration (environment)

| Variable | Default | |
|---|---|---|
| `DATABASE_URL` | required | Postgres connection string |
| `PORT` / `HOST` | `3000` / `0.0.0.0` | |
| `DB_MAX_CONNECTIONS` | `10` | pool size per process |
| `WORKER_CONCURRENCY` | `8` | concurrent jobs per process |
| `JOBS_LISTEN` | `true` | set `false` behind PgBouncer transaction pooling |
| `MIGRATE_ON_START` | `true` | |
| `REQUEST_TIMEOUT_SECS` | `30` | |
| `CPU_THREADS` | cores − 1 | size of the CPU pool for heavy work |
| `LOG_FORMAT` | `json` | `pretty` for development |
| `RUST_LOG` | `info` | |

Endpoints: `POST /rpc/{name}`, `GET /healthz` (liveness), `GET /readyz` (database
reachable), and everything else goes to the embedded frontend.

## Scaling

The binary is stateless. All state, including the job queue, lives in Postgres.
Run as many replicas as you like. To scale HTTP and jobs separately, deploy the
same image twice: `app serve` and `app worker`.

## For agents

Read [CLAUDE.md](CLAUDE.md). Loop on `just check`; finish with `just verify`.
