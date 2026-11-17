<h1 align="center">northstack</h1>

<p align="center">
  <strong>Rust, React and SQLite in one binary, typed end to end.</strong>
</p>

<p align="center">
  <a href="#try-it">Try it</a> ·
  <a href="crates/northstack/GUIDE.md">The guide</a> ·
  <a href="#updating">Updating</a> ·
  <a href="#working-on-northstack">Contributing</a>
</p>

---

northstack is an open-source, full-stack web framework: a **Rust** backend
(axum, sqlx), a **React** frontend with Next.js-style folder routing, and
**SQLite**.

- **Typed end to end.** Types are generated from your SQLite schema: the
  tables become Rust types, your Rust procedures become a typed TypeScript
  client (like tRPC), and every SQL query is checked at compile time. Rename a
  column and `just check` shows every query and page it breaks.
- **One binary.** The server, the frontend and the database engine build into
  one file you copy to a server, like PocketBase. No database server to run.
- **Built for coding agents.** Claude Code, Codex and Cursor write and maintain
  northstack apps: there's one obvious way to do everything, and the compiler
  catches mistakes before you ever see them.

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
  wrong. The guide is tested in CI, so it always matches the code.

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
cargo install northstack --locked
northstack new my-app && cd my-app
just dev        # open http://localhost:5173
```

You'll see a small notes app. It's an **example** that shows every pattern
once; you replace it with your own features (`just remove-example` gives you an
empty app).

Then point your coding agent at the app. It reads `AGENTS.md`, which sends it to
`docs/northstack.md`: [the guide](crates/northstack/GUIDE.md) for the exact
framework version the app uses.

## Updating

An app is your code plus one dependency: the `northstack` crate, pinned to an
exact version in `Cargo.toml`. The crate holds everything that isn't yours: the
server library, the TypeScript client and router (written into `web/northstack/`
by `just check`), the guide, and the commands behind `just`. Nothing changes
until you run:

```sh
just upgrade    # or: just upgrade 0.2.0
```

It moves the pin, replaces `docs/northstack.md`, prints the upgrade notes for
every version it skipped (from the [changelog](crates/northstack/CHANGELOG.md),
written as steps an agent can follow), and runs `just check`, so anything that
no longer compiles shows up right away.

## Working on northstack

This repository is the framework (`crates/northstack`) and the example app
(`server/`, `web/`), which is also the template `northstack new` copies. It is
itself a runnable app, using the framework from the path:

```sh
git clone https://github.com/elitan/northstack && cd northstack
just dev
```

[AGENTS.md](AGENTS.md) has the rules for changing the framework and how to
release a version.
