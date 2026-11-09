//! Scheduled jobs that are safe to run on every replica.
//!
//! A cron entry never does work itself. At each scheduled time, every replica
//! tries to enqueue a job with the idempotency key `cron:{name}:{timestamp}`.
//! The unique index lets exactly one insert succeed, and a normal worker runs
//! the job with retries and logging. No leader election needed.
//!
//! Expressions use the `cron` crate syntax with a seconds field, in UTC:
//! `"0 0 3 * * *"` = every day at 03:00:00 UTC.
//!
//! If every replica is down at a scheduled time, that run is skipped.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::jobs::{EnqueueOptions, Job, enqueue_raw};

type MakePayload = Arc<dyn Fn(DateTime<Utc>) -> serde_json::Value + Send + Sync>;

#[derive(Clone)]
struct Entry {
    name: &'static str,
    schedule: cron::Schedule,
    kind: &'static str,
    max_attempts: i32,
    timeout: Duration,
    make: MakePayload,
}

#[derive(Default, Clone)]
pub struct Cron {
    entries: Vec<Entry>,
}

impl Cron {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue the job returned by `make` on the given schedule. `make`
    /// receives the scheduled time, useful for "report for yesterday" jobs.
    ///
    /// Panics on an invalid expression or duplicate name, so mistakes fail at
    /// startup and in tests.
    pub fn add<J: Job>(
        mut self,
        name: &'static str,
        expr: &str,
        make: impl Fn(DateTime<Utc>) -> J + Send + Sync + 'static,
    ) -> Self {
        let schedule = cron::Schedule::from_str(expr)
            .unwrap_or_else(|e| panic!("invalid cron expression {expr:?} for {name:?}: {e}"));
        assert!(
            self.entries.iter().all(|e| e.name != name),
            "cron entry {name:?} registered twice"
        );
        self.entries.push(Entry {
            name,
            schedule,
            kind: J::KIND,
            max_attempts: J::MAX_ATTEMPTS,
            timeout: J::TIMEOUT,
            make: Arc::new(move |at| {
                serde_json::to_value(make(at)).expect("cron job payload must serialize")
            }),
        });
        self
    }

    pub fn merge(mut self, other: Cron) -> Self {
        for e in other.entries {
            assert!(
                self.entries.iter().all(|x| x.name != e.name),
                "cron entry {:?} registered twice",
                e.name
            );
            self.entries.push(e);
        }
        self
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.entries.iter().map(|e| e.name).collect()
    }

    /// Enqueue the run for `name` scheduled at `at`, exactly as the scheduler
    /// would. Returns whether this call inserted it. Useful in tests.
    pub async fn fire(&self, pool: &PgPool, name: &str, at: DateTime<Utc>) -> anyhow::Result<bool> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| anyhow::anyhow!("no cron entry {name:?}"))?;
        fire_entry(pool, entry, at).await
    }

    /// Run the scheduler until `shutdown` is cancelled.
    pub async fn run(self, pool: PgPool, shutdown: CancellationToken) {
        if self.entries.is_empty() {
            return;
        }
        tracing::info!(entries = ?self.names(), "cron started");
        let mut tasks = Vec::new();
        for entry in self.entries {
            let (pool, shutdown) = (pool.clone(), shutdown.clone());
            tasks.push(tokio::spawn(async move {
                loop {
                    let Some(next) = entry.schedule.upcoming(Utc).next() else { return };
                    let wait = (next - Utc::now()).to_std().unwrap_or_default();
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => {}
                        _ = shutdown.cancelled() => return,
                    }
                    match fire_entry(&pool, &entry, next).await {
                        Ok(true) => tracing::info!(cron = entry.name, at = %next, "cron enqueued"),
                        Ok(false) => tracing::debug!(cron = entry.name, at = %next, "cron already enqueued by another replica"),
                        Err(e) => tracing::error!(cron = entry.name, error = %e, "cron enqueue failed"),
                    }
                    // Make sure we move past this slot even with clock jitter.
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }));
        }
        for t in tasks {
            let _ = t.await;
        }
    }
}

async fn fire_entry(pool: &PgPool, entry: &Entry, at: DateTime<Utc>) -> anyhow::Result<bool> {
    let key = format!("cron:{}:{}", entry.name, at.timestamp());
    let inserted = enqueue_raw(
        pool,
        entry.kind,
        (entry.make)(at),
        entry.max_attempts,
        entry.timeout,
        EnqueueOptions {
            run_at: None,
            run_key: Some(key),
        },
    )
    .await?;
    Ok(inserted.is_some())
}
