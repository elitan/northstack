//! Database types. `schema.rs` is generated from Postgres by `northstack gen-db`
//! (run `just check`); never edit it by hand.

mod schema;

pub use schema::*;
