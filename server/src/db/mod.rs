//! Database types. `schema.rs` is generated from the database by `northstack gen-db`
//! (run `just check`); never edit it by hand.

mod schema;

// Allowed so an app without tables yet (e.g. after `just remove-example`) has no warning.
#[allow(unused_imports)]
pub use schema::*;
