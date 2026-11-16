# northstack

Full-stack web framework for AI coding agents: a Rust backend (axum, sqlx), a
React frontend with Next.js-style folder routing, typed RPC in between, and
SQLite, shipped as one small binary.

```sh
cargo install northstack --locked
northstack new my-app && cd my-app
just dev        # http://localhost:5173
```

This crate is both the framework (the library your app's server depends on)
and the `northstack` command. An app pins one exact version in its
`Cargo.toml`; `just upgrade` moves it to a newer one.

Documentation: the guide ([GUIDE.md](GUIDE.md), written into every app as
`docs/northstack.md`) and <https://github.com/elitan/northstack>.
