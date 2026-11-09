//! Durable background jobs stored in Postgres.
//!
//! - Jobs are typed Rust structs implementing [`Job`]; the kind string is a
//!   constant on the type, so a job can't be enqueued under a misspelled name.
//! - [`enqueue`] takes any executor, so a job can be enqueued inside the same
//!   transaction as the business write: both commit or neither does.
//! - Workers claim jobs with `FOR UPDATE SKIP LOCKED`, so any number of
//!   replicas can run workers without double-processing.
//! - Each claim takes a lease (`Job::TIMEOUT`). If a replica crashes, the lease
//!   expires and another worker retries the job.
//! - Failures retry with exponential backoff up to `Job::MAX_ATTEMPTS`, then
//!   the job is marked `dead` and kept for inspection.
//! - Enqueue sends `NOTIFY northstack_jobs` (delivered on commit) so idle workers
//!   wake immediately; a slow poll is the fallback.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::{PgExecutor, PgPool};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

/// A background job. Implement this on a serializable struct.
///
/// ```ignore
/// #[derive(Serialize, Deserialize)]
/// pub struct SendWelcomeEmail { pub user_id: Uuid }
///
/// impl northstack::Job for SendWelcomeEmail {
///     const KIND: &'static str = "users.send_welcome_email";
/// }
/// ```
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable identifier stored in the database. Never change it once jobs exist.
    const KIND: &'static str;
    /// Total attempts before the job is marked `dead`.
    const MAX_ATTEMPTS: i32 = 5;
    /// Maximum run time per attempt. Also the lease: if a worker dies, the job
    /// becomes claimable again after this long.
    const TIMEOUT: Duration = Duration::from_secs(300);
}

/// Options for [`enqueue_with`].
#[derive(Debug, Clone, Default)]
pub struct EnqueueOptions {
    /// Run no earlier than this time. Defaults to now.
    pub run_at: Option<DateTime<Utc>>,
    /// Idempotency key. A second enqueue with the same key is ignored.
    pub run_key: Option<String>,
}

/// Enqueue a job to run as soon as possible. Returns the job id.
pub async fn enqueue<'e, J: Job>(db: impl PgExecutor<'e>, job: &J) -> sqlx::Result<i64> {
    let id = enqueue_with(db, job, EnqueueOptions::default()).await?;
    Ok(id.expect("enqueue without run_key always inserts"))
}

/// Enqueue with options. Returns `None` if a job with the same `run_key`
/// already exists.
pub async fn enqueue_with<'e, J: Job>(
    db: impl PgExecutor<'e>,
    job: &J,
    opts: EnqueueOptions,
) -> sqlx::Result<Option<i64>> {
    let payload = serde_json::to_value(job).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    enqueue_raw(db, J::KIND, payload, J::MAX_ATTEMPTS, J::TIMEOUT, opts).await
}

pub(crate) async fn enqueue_raw<'e>(
    db: impl PgExecutor<'e>,
    kind: &str,
    payload: serde_json::Value,
    max_attempts: i32,
    timeout: Duration,
    opts: EnqueueOptions,
) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        WITH ins AS (
            INSERT INTO northstack_jobs (kind, payload, run_at, run_key, max_attempts, timeout_secs)
            VALUES ($1, $2, COALESCE($3, now()), $4, $5, $6)
            ON CONFLICT (run_key) WHERE run_key IS NOT NULL DO NOTHING
            RETURNING id
        )
        SELECT ins.id FROM ins CROSS JOIN LATERAL (SELECT pg_notify('northstack_jobs', '')) AS n
        "#,
    )
    .bind(kind)
    .bind(payload)
    .bind(opts.run_at)
    .bind(opts.run_key)
    .bind(max_attempts.max(1))
    .bind(timeout.as_secs().clamp(1, i32::MAX as u64) as i32)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.0))
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler<S> = Arc<dyn Fn(S, serde_json::Value) -> BoxFuture<anyhow::Result<()>> + Send + Sync>;

/// Registry of job handlers. Build one per feature and [`Jobs::merge`] them.
#[derive(Clone)]
pub struct Jobs<S> {
    handlers: HashMap<&'static str, Handler<S>>,
}

impl<S> Default for Jobs<S> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<S: Clone + Send + Sync + 'static> Jobs<S> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the handler for job type `J`. Handlers must be idempotent:
    /// a job can run more than once if a worker crashes mid-way.
    pub fn register<J, F, Fut>(mut self, handler: F) -> Self
    where
        J: Job,
        F: Fn(S, J) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let h: Handler<S> = Arc::new(move |state, payload| {
            let handler = handler.clone();
            Box::pin(async move {
                let job: J = serde_json::from_value(payload)
                    .map_err(|e| anyhow::anyhow!("invalid payload for {}: {e}", J::KIND))?;
                handler(state, job).await
            })
        });
        let previous = self.handlers.insert(J::KIND, h);
        assert!(
            previous.is_none(),
            "job kind {:?} registered twice",
            J::KIND
        );
        self
    }

    pub fn merge(mut self, other: Jobs<S>) -> Self {
        for (kind, h) in other.handlers {
            let previous = self.handlers.insert(kind, h);
            assert!(previous.is_none(), "job kind {kind:?} registered twice");
        }
        self
    }

    pub fn kinds(&self) -> Vec<&'static str> {
        let mut k: Vec<_> = self.handlers.keys().copied().collect();
        k.sort();
        k
    }

    /// Run all jobs that are ready now, one at a time, until none are left.
    /// Returns how many attempts were made. Intended for tests.
    pub async fn run_pending(&self, pool: &PgPool, state: S) -> anyhow::Result<usize> {
        let mut ran = 0;
        loop {
            let claimed = claim(pool, 1).await?;
            let Some(job) = claimed.into_iter().next() else {
                return Ok(ran);
            };
            ran += 1;
            self.execute(pool, state.clone(), job).await;
        }
    }

    async fn execute(&self, pool: &PgPool, state: S, job: ClaimedJob) {
        let outcome: anyhow::Result<()> = if job.attempts > job.max_attempts {
            Err(anyhow::anyhow!(
                "lease expired after final attempt (worker crashed or timed out)"
            ))
        } else if let Some(handler) = self.handlers.get(job.kind.as_str()).cloned() {
            let timeout = Duration::from_secs(job.timeout_secs.max(1) as u64);
            let span =
                tracing::info_span!("job", id = job.id, kind = %job.kind, attempt = job.attempts);
            // Run in its own task so a panic is contained and reported.
            let mut task = tokio::spawn(tracing::Instrument::instrument(
                handler(state, job.payload.clone()),
                span,
            ));
            match tokio::time::timeout(timeout, &mut task).await {
                Ok(Ok(result)) => result,
                Ok(Err(join)) if join.is_panic() => Err(anyhow::anyhow!("job panicked")),
                Ok(Err(join)) => Err(anyhow::anyhow!("job task failed: {join}")),
                Err(_) => {
                    task.abort();
                    Err(anyhow::anyhow!("timed out after {}s", timeout.as_secs()))
                }
            }
        } else {
            // Unknown kind: retrying won't help.
            let msg = format!("no handler registered for kind {:?}", job.kind);
            if let Err(e) = mark_dead(pool, job.id, &msg).await {
                tracing::error!(error = %e, job_id = job.id, "failed to mark job dead");
            }
            tracing::error!(job_id = job.id, kind = %job.kind, "{msg}");
            return;
        };

        let result = match outcome {
            Ok(()) => {
                tracing::info!(job_id = job.id, kind = %job.kind, "job done");
                mark_done(pool, job.id).await
            }
            Err(err) => {
                let msg = format!("{err:#}");
                if job.attempts >= job.max_attempts {
                    tracing::error!(job_id = job.id, kind = %job.kind, error = %msg, "job dead");
                    mark_dead(pool, job.id, &msg).await
                } else {
                    let delay = backoff(job.attempts);
                    tracing::warn!(job_id = job.id, kind = %job.kind, error = %msg, retry_in_s = delay.as_secs(), "job failed; will retry");
                    mark_retry(pool, job.id, &msg, delay).await
                }
            }
        };
        if let Err(e) = result {
            tracing::error!(error = %e, job_id = job.id, "failed to record job result");
        }
    }
}

/// Exponential backoff: 10s, 20s, 40s, ... capped at one hour.
pub fn backoff(attempt: i32) -> Duration {
    let exp = attempt.clamp(1, 20) as u32 - 1;
    Duration::from_secs((10u64 << exp).min(3600))
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimedJob {
    id: i64,
    kind: String,
    payload: serde_json::Value,
    attempts: i32,
    max_attempts: i32,
    timeout_secs: i32,
}

async fn claim(pool: &PgPool, limit: i64) -> sqlx::Result<Vec<ClaimedJob>> {
    sqlx::query_as(
        r#"
        UPDATE northstack_jobs j
        SET status = 'running',
            attempts = j.attempts + 1,
            locked_until = now() + make_interval(secs => j.timeout_secs + 30),
            updated_at = now()
        WHERE j.id IN (
            SELECT id FROM northstack_jobs
            WHERE (status = 'queued' AND run_at <= now())
               OR (status = 'running' AND locked_until < now())
            ORDER BY run_at, id
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING j.id, j.kind, j.payload, j.attempts, j.max_attempts, j.timeout_secs
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

async fn mark_done(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE northstack_jobs SET status = 'done', locked_until = NULL, last_error = NULL,
             finished_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn mark_retry(pool: &PgPool, id: i64, error: &str, delay: Duration) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE northstack_jobs SET status = 'queued', locked_until = NULL, last_error = $2,
             run_at = now() + make_interval(secs => $3), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .bind(delay.as_secs_f64())
    .execute(pool)
    .await
    .map(|_| ())
}

async fn mark_dead(pool: &PgPool, id: i64, error: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE northstack_jobs SET status = 'dead', locked_until = NULL, last_error = $2,
             finished_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Worker settings.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Maximum jobs running at once in this process.
    pub concurrency: usize,
    /// Fallback poll interval when no notification arrives.
    pub poll_interval: Duration,
    /// Use `LISTEN northstack_jobs` for instant wake-up. Disable when connecting
    /// through PgBouncer in transaction mode (LISTEN needs a session).
    pub listen: bool,
    /// Delete finished (`done`) jobs older than this.
    pub retention: Duration,
    /// How long to wait for running jobs on shutdown.
    pub shutdown_grace: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 8,
            poll_interval: Duration::from_secs(2),
            listen: true,
            retention: Duration::from_secs(7 * 24 * 3600),
            shutdown_grace: Duration::from_secs(30),
        }
    }
}

/// Run workers until `shutdown` is cancelled, then wait for running jobs.
pub async fn run_workers<S: Clone + Send + Sync + 'static>(
    pool: PgPool,
    jobs: Arc<Jobs<S>>,
    state: S,
    config: WorkerConfig,
    shutdown: CancellationToken,
) {
    let concurrency = config.concurrency.max(1);
    let permits = Arc::new(Semaphore::new(concurrency));
    let wake = Arc::new(Notify::new());

    if config.listen {
        tokio::spawn(listen_loop(pool.clone(), wake.clone(), shutdown.clone()));
    }
    tokio::spawn(cleanup_loop(
        pool.clone(),
        config.retention,
        shutdown.clone(),
    ));
    tracing::info!(concurrency, kinds = ?jobs.kinds(), "workers started");

    loop {
        // Wait for at least one free slot.
        let first = tokio::select! {
            p = permits.clone().acquire_owned() => p.expect("semaphore never closed"),
            _ = shutdown.cancelled() => break,
        };
        let mut held = vec![first];
        while let Ok(p) = permits.clone().try_acquire_owned() {
            held.push(p);
        }

        match claim(&pool, held.len() as i64).await {
            Ok(claimed) if !claimed.is_empty() => {
                for job in claimed {
                    let permit = held.pop().expect("claimed at most held permits");
                    let (jobs, pool, state) = (jobs.clone(), pool.clone(), state.clone());
                    tokio::spawn(async move {
                        jobs.execute(&pool, state, job).await;
                        drop(permit);
                    });
                }
                drop(held);
                // There may be more ready work; loop immediately.
                continue;
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to claim jobs"),
        }
        drop(held);

        tokio::select! {
            _ = wake.notified() => {}
            _ = tokio::time::sleep(config.poll_interval) => {}
            _ = shutdown.cancelled() => break,
        }
    }

    tracing::info!("workers stopping; waiting for running jobs");
    let all = permits.acquire_many(concurrency as u32);
    if tokio::time::timeout(config.shutdown_grace, all)
        .await
        .is_err()
    {
        tracing::warn!(
            "shutdown grace period elapsed with jobs still running; their leases will expire and they will be retried"
        );
    }
}

async fn listen_loop(pool: PgPool, wake: Arc<Notify>, shutdown: CancellationToken) {
    loop {
        let result: sqlx::Result<()> = async {
            let mut listener = sqlx::postgres::PgListener::connect_with(&pool).await?;
            listener.listen("northstack_jobs").await?;
            loop {
                tokio::select! {
                    n = listener.recv() => { n?; wake.notify_one(); }
                    _ = shutdown.cancelled() => return Ok(()),
                }
            }
        }
        .await;
        match result {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!(error = %e, "job listener disconnected; falling back to polling and retrying");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = shutdown.cancelled() => return,
                }
            }
        }
    }
}

async fn cleanup_loop(pool: PgPool, retention: Duration, shutdown: CancellationToken) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = shutdown.cancelled() => return,
        }
        let res = sqlx::query(
            "DELETE FROM northstack_jobs WHERE status = 'done' AND finished_at < now() - make_interval(secs => $1)",
        )
        .bind(retention.as_secs_f64())
        .execute(&pool)
        .await;
        match res {
            Ok(r) if r.rows_affected() > 0 => {
                tracing::info!(deleted = r.rows_affected(), "cleaned up finished jobs")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "job cleanup failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::backoff;
    use std::time::Duration;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(10));
        assert_eq!(backoff(2), Duration::from_secs(20));
        assert_eq!(backoff(3), Duration::from_secs(40));
        assert_eq!(backoff(50), Duration::from_secs(3600));
    }
}
