//! Notes: the reference feature. Copy this layout for new features:
//!
//! - `types.rs`       inputs/outputs shared with TypeScript (`#[derive(TS)]`)
//! - `db.rs`          all SQL for the feature (compile-time checked)
//! - `procedures.rs`  RPC handlers: validate, call db, enqueue jobs
//! - `jobs.rs`        background jobs and cron entries

mod db;
mod jobs;
mod procedures;
pub mod types;

pub use jobs::{AnalyzeNote, DailyDigest};

use crate::AppState;
use northstack::{Cron, Jobs, Rpc};

pub fn rpc() -> Rpc<AppState> {
    Rpc::new()
        .query("notes.list", procedures::list)
        .query("notes.get", procedures::get)
        .query("notes.stats", procedures::stats)
        .mutation("notes.create", procedures::create)
        .mutation("notes.update", procedures::update)
        .mutation("notes.delete", procedures::delete)
}

pub fn jobs() -> Jobs<AppState> {
    Jobs::new()
        .register(jobs::analyze_note)
        .register(jobs::daily_digest)
}

pub fn cron() -> Cron {
    // Every day at 07:00:00 UTC, write yesterday's digest.
    Cron::new().add("notes.daily_digest", "0 0 7 * * *", |at| DailyDigest {
        day: (at - chrono::Duration::days(1)).date_naive(),
    })
}
