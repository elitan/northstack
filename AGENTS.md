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

## Shipping a change

The human prompts; you do the whole flow. Work on `main` and push to it, no
pull requests:

1. Start from an up-to-date `main` (`git pull`).
2. Make the change, with tests. If it reaches apps (see "The changelog"), add
   its line under `## Unreleased` in `crates/northstack/CHANGELOG.md` in the
   same commit.
3. Before pushing, all of these pass: `just verify`, `just e2e` (if pages
   changed), `python3 scripts/guide_test.py` (if the guide, template, CLI or
   scripts changed) and `python3 scripts/changelog_check.py origin/main`.
4. Commit and `git push origin main`.
5. Watch CI on that commit: `gh run list --branch main --limit 3`, then
   `gh run watch <id>`. If it fails, read the log
   (`gh run view <id> --log-failed`) and push a fix right away; `main` must
   never stay red.

Never: force-push `main`, rewrite or delete a `v*` tag, run `cargo publish` by
hand, or push while a check you could run locally is failing. If something
needs one of these, stop and ask the human.

## The changelog

`crates/northstack/CHANGELOG.md` is what app agents read when they upgrade
(`just upgrade` prints it), so write it for them:

- A change **reaches apps** if it touches the crate (`crates/northstack/`, not
  its tests) or a file `northstack new` copies that the framework keeps
  maintaining (`justfile`, `Dockerfile`, `.github/workflows/ci.yml`, `cli/`,
  `web/` config, ...; the list is `APP_FILES` in `scripts/changelog_check.py`).
  It needs a line under `## Unreleased`: what changed, then `**Upgrade:**`
  with the exact steps an app needs ("rename `x` to `y` in
  `server/src/features/*/procedures.rs`", "copy this block into your
  Dockerfile"), or `**Upgrade:** nothing to do.`
- Changes to tests, docs, benchmarks or the example feature need nothing. A
  pure refactor of the crate with no visible effect: put `[no changelog]` in
  the commit message.
- Released sections never change.

CI (`framework.yml`, job `changelog`) fails a push to `main` that changes
something apps get without touching the changelog.

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
scripts/release.py        Releases a version (see "Releasing a version")
scripts/release_notes.py  Checks a release tag and prints its changelog section
scripts/changelog_check.py CI: changelog lines for changes that reach apps
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

Prefer changes apps don't have to react to. When one is worth it (a renamed
API, a new required file, a changed command), its `**Upgrade:**` steps in the
changelog must be complete enough that an agent in an app can follow them
without reading this repository.

## Releasing a version

When the human asks for a release ("release", "ship a new version"):

1. Pick the bump from `## Unreleased`: any `**Upgrade:**` step that asks apps
   to change something makes it `minor` before 1.0 (`major` after);
   otherwise `patch`. (The first release is the crate's current version,
   whatever you pass.)
2. On a clean, up-to-date `main`: `python3 scripts/release.py <patch|minor|major>`.
   It refuses an empty `Unreleased` and a patch with upgrade steps, moves
   `Unreleased` into the new version's section, bumps the crate version,
   commits `Release v<version>` and pushes it to `main`.
3. That push starts `.github/workflows/release.yml`: all checks, then
   `cargo publish`, then the `v<version>` tag and GitHub release with the
   changelog section as notes. Watch it: `gh run list --workflow release.yml
   --limit 1`, then `gh run watch <id>`.
4. Confirm: `gh release view v<version>` and `cargo search northstack --limit 1`
   show the version. Report both links to the human.

If the release workflow fails, read the log, push a fix to `main` (that push
retries the release), or re-run it with
`gh workflow run release.yml`. A version that's already on crates.io isn't
published again. A published version can't be replaced; ship a new patch
instead.

**crates.io access (the human, once).** Publishing needs the repository secret
`CARGO_REGISTRY_TOKEN`. If it's missing or the publish step fails with an
authentication error, stop and ask the human to create a crates.io API token
(scopes `publish-new` and `publish-update`) and run
`gh secret set CARGO_REGISTRY_TOKEN --repo elitan/northstack`. Never ask for
the token in chat or handle it yourself.
