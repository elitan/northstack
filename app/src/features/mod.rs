//! One module per feature. Each exposes `rpc()`, and optionally `jobs()` and
//! `cron()`, which are registered in `crate::build`.

pub mod notes;
