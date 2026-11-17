# {{name}}

A [northstack](https://github.com/elitan/northstack) app: Rust (axum, sqlx) on
the server, React with folder routing in the browser, typed RPC in between, and
SQLite, shipped as one binary.

```sh
just dev        # api :3000 and Vite :5173, in the background
just check      # after every change
just verify     # before committing
```

You need Rust, Bun and `just` (`brew install rustup bun just && rustup default stable`).

- [docs/northstack.md](docs/northstack.md): the framework guide (commands,
  building a feature, database, testing, deploying).
- [AGENTS.md](AGENTS.md): the rules coding agents follow here.
- `just upgrade`: move to the latest northstack version.

Deploy: `docker build -t app .` gives one image with one binary; the guide's
"Running in production" has the details.
