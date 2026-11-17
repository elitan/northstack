# northstack

Rust, React and SQLite in one binary, typed end to end.

A full-stack web framework: a Rust backend (axum, sqlx), a React frontend with
Next.js-style folder routing, and SQLite. SQL is checked at compile time, the
schema becomes Rust types, and Rust procedures become a typed TypeScript
client. The whole app ships as one binary. Built for apps that AI coding
agents write and maintain.

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
