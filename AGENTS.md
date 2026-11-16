# AGENTS.md

This repository is the northstack framework and its example app:

- `crates/northstack/`: the framework, published to crates.io as `northstack`.
  Apps depend on one exact version of it.
- `server/`, `web/`, `cli/` and the files around them: the example "notes" app.
  It is also the **template**: `northstack new` copies it (every tracked file
  except the framework's, see `is_framework_file` in `crates/northstack/src/cli.rs`).

The example app uses the framework from the path, so it is a normal northstack
app: `just dev`, `just check`, `just verify` and `just e2e` work here as in any
app. When you change `server/` or `web/`, follow the app rules in
[crates/northstack/src/template/AGENTS.md](crates/northstack/src/template/AGENTS.md)
(apps get that file as their `AGENTS.md`) and the guide,
[crates/northstack/GUIDE.md](crates/northstack/GUIDE.md) (apps get it as
`docs/northstack.md`). Read both before your first change.

## The loop

```sh
just check                      # after every change
just verify                     # before you're done: + fmt, clippy, all tests, build
just e2e                        # if pages changed
python3 scripts/guide_test.py   # if the guide, template, CLI or scripts changed (CI runs it)
```

`scripts/guide_test.py` creates an app with `northstack new --local .`, checks
it, removes the example, then runs the guide's "Building a feature" walkthrough
exactly as written (the `<!-- guide-test: ... -->` markers) and `just verify`
on the result. It also checks that every path the docs mention exists.

## The framework

```
crates/northstack/
  src/app.rs              App builder, CLI modes, HTTP router, config from env
  src/rpc.rs              Rpc registry: .query()/.mutation(), Ctx, POST /rpc/{name}
  src/error.rs            RpcError, ErrorCode, ensure! macro
  src/db.rs               SQLite settings, db::begin (write transactions), db::NOW
  src/jobs.rs             Job queue in the database: Job trait, Jobs registry, enqueue, workers
  src/cron.rs             Cron schedules that enqueue jobs, once per slot
  src/cpu.rs              Thread pool for CPU-heavy work (cpu::spawn)
  src/schema.rs           gen-db: SQLite schema -> Rust structs, enums and sqlx.toml
  src/codegen.rs          gen-ts: Rust procedures -> TypeScript client
  src/ts/*.ts             The TypeScript runtime, written into web/northstack/ by gen-ts
  src/testing.rs          Test databases (testing::db) and an in-process HTTP client
  src/web.rs              Serves the embedded frontend (index.html fallback)
  src/cli.rs              The `northstack` command: new, upgrade, db commands; runs the scripts
  src/scripts/*.sh        check, dev, e2e, new-feature, remove-example (embedded in the CLI)
  src/template/           AGENTS.md and README.md that `northstack new` writes
  src/bin/northstack.rs   The globally installed command (`cargo install northstack`)
  migrations/             The framework's own table (northstack_jobs)
  tests/                  Framework tests (jobs, database, schema generation, codegen)
  GUIDE.md                The guide; apps get it as docs/northstack.md
  CHANGELOG.md            Every release, with upgrade steps; `just upgrade` prints them
scripts/guide_test.py     Tests `northstack new` and the guide (see above)
scripts/release_notes.py  Checks a release tag and prints its changelog section
.github/workflows/        ci.yml (also copied into apps), framework.yml, release.yml
bench/                    Benchmark against Next.js + Postgres (bench/README.md)
```

Everything an app needs from the framework ships in the crate, so it updates
with the version: the library, the TypeScript runtime, the scripts behind
`just`, and the guide. Files `northstack new` copies (the `justfile`,
`Dockerfile`, `.github/workflows/ci.yml`, `web/` config) belong to the app once
created and never update, so keep them thin: put logic in the CLI instead.

## Changing the framework

`crates/northstack/src` is small and fully tested (`crates/northstack/tests`,
plus unit tests in `cli.rs`). Change it when a pattern repeats across features,
keep its public API small, and add a test for every behavior change. Apps
should only need what `lib.rs` exports: `App`, `Rpc`, `Ctx`, `RpcResult`,
`RpcError`, `ErrorCode`, `ensure!`, `Job`, `Jobs`, `enqueue`, `enqueue_with`,
`EnqueueOptions`, `Cron`, `db`, `cpu`, `testing`, and the re-exported crates
(`sqlx`, `axum`, `serde`, `chrono`, `uuid`, `ts_rs`, `tokio`, `tracing`).

Every change an app has to react to (a renamed API, a new required file, a
changed command) goes in `CHANGELOG.md` under the next version's **Upgrade**
heading, as exact steps an agent can follow in an app.

## Releasing a version

1. Set the version in `crates/northstack/Cargo.toml` and add its section to
   `crates/northstack/CHANGELOG.md` (what changed, and the **Upgrade** steps,
   or "nothing to do"). Check both with
   `python3 scripts/release_notes.py v<version>`.
2. `just verify`, `just e2e` and `python3 scripts/guide_test.py` pass. Commit
   and merge to `main`.
3. Tag the merged commit and push the tag:
   `git tag v<version> && git push origin v<version>`.

The tag starts `.github/workflows/release.yml`: it checks the tag matches the
crate and has a changelog section, runs `ci.yml` and `framework.yml`, publishes
the crate to crates.io (trusted publishing, no token) and creates the GitHub
release with the changelog section as notes. `northstack new` downloads the
template from the tag, so the tag always exists before the crate does.

If the workflow fails before publishing, fix it on `main`, then move the tag:
`git tag -f v<version> && git push -f origin v<version>`. Once a version is on
crates.io it can't be replaced; release a new patch version instead.

**First release only** (trusted publishing needs the crate to exist): publish
0.1.0 by hand, `cargo login` then `cargo publish -p northstack`, after pushing
the `v0.1.0` tag. Then on crates.io, under the crate's Settings -> Trusted
Publishing, add repository `elitan/northstack`, workflow `release.yml`,
environment `release`.
