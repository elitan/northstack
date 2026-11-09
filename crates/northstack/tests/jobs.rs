//! Job queue and cron behavior against a real Postgres. Each test gets a fresh
//! database from `#[sqlx::test]`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use northstack::{EnqueueOptions, Job, Jobs, WorkerConfig};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use tokio_util::sync::CancellationToken;

static NO_APP_MIGRATIONS: Migrator = Migrator::DEFAULT;

async fn setup(pool: &PgPool) {
    northstack::migrate(pool, &NO_APP_MIGRATIONS).await.unwrap();
    sqlx::query("CREATE TABLE runs (job_id INT NOT NULL)")
        .execute(pool)
        .await
        .unwrap();
}

#[derive(Clone)]
struct State {
    pool: PgPool,
    calls: Arc<AtomicUsize>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    n: i32,
}
impl Job for Record {
    const KIND: &'static str = "test.record";
}

async fn record(state: State, job: Record) -> anyhow::Result<()> {
    state.calls.fetch_add(1, Ordering::SeqCst);
    sqlx::query("INSERT INTO runs (job_id) VALUES ($1)")
        .bind(job.n)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct AlwaysFails;
impl Job for AlwaysFails {
    const KIND: &'static str = "test.always_fails";
    const MAX_ATTEMPTS: i32 = 3;
}

#[derive(Serialize, Deserialize)]
struct Panics;
impl Job for Panics {
    const KIND: &'static str = "test.panics";
    const MAX_ATTEMPTS: i32 = 1;
}

#[derive(Serialize, Deserialize)]
struct Slow;
impl Job for Slow {
    const KIND: &'static str = "test.slow";
    const MAX_ATTEMPTS: i32 = 1;
    const TIMEOUT: Duration = Duration::from_secs(1);
}

fn registry() -> Jobs<State> {
    Jobs::new()
        .register(record)
        .register(|_s: State, _j: AlwaysFails| async { anyhow::bail!("nope") })
        .register(|_s: State, _j: Panics| async { panic!("boom") })
        .register(|_s: State, _j: Slow| async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        })
}

fn state(pool: &PgPool) -> State {
    State {
        pool: pool.clone(),
        calls: Arc::new(AtomicUsize::new(0)),
    }
}

async fn status_of(pool: &PgPool, id: i64) -> (String, i32, Option<String>) {
    sqlx::query_as("SELECT status, attempts, last_error FROM northstack_jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Make a job that is waiting for its backoff ready right away.
async fn skip_backoff(pool: &PgPool) {
    sqlx::query("UPDATE northstack_jobs SET run_at = now() WHERE status = 'queued'")
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = false)]
async fn runs_a_job_once(pool: PgPool) {
    setup(&pool).await;
    let id = northstack::enqueue(&pool, &Record { n: 1 }).await.unwrap();
    let s = state(&pool);
    assert_eq!(registry().run_pending(&pool, s.clone()).await.unwrap(), 1);
    assert_eq!(s.calls.load(Ordering::SeqCst), 1);
    assert_eq!(status_of(&pool, id).await.0, "done");
    // Nothing left.
    assert_eq!(registry().run_pending(&pool, s).await.unwrap(), 0);
}

#[sqlx::test(migrations = false)]
async fn enqueue_is_transactional(pool: PgPool) {
    setup(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    northstack::enqueue(&mut *tx, &Record { n: 1 })
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM northstack_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rolled-back enqueue must not leave a job");
}

#[sqlx::test(migrations = false)]
async fn run_key_deduplicates(pool: PgPool) {
    setup(&pool).await;
    let opts = || EnqueueOptions {
        run_key: Some("once".into()),
        ..Default::default()
    };
    assert!(
        northstack::enqueue_with(&pool, &Record { n: 1 }, opts())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        northstack::enqueue_with(&pool, &Record { n: 2 }, opts())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = false)]
async fn scheduled_jobs_wait(pool: PgPool) {
    setup(&pool).await;
    let later = chrono::Utc::now() + chrono::Duration::hours(1);
    northstack::enqueue_with(
        &pool,
        &Record { n: 1 },
        EnqueueOptions {
            run_at: Some(later),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        registry().run_pending(&pool, state(&pool)).await.unwrap(),
        0
    );
}

#[sqlx::test(migrations = false)]
async fn failures_retry_with_backoff_then_die(pool: PgPool) {
    setup(&pool).await;
    let id = northstack::enqueue(&pool, &AlwaysFails).await.unwrap();
    let jobs = registry();

    jobs.run_pending(&pool, state(&pool)).await.unwrap();
    let (status, attempts, err) = status_of(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("queued", 1));
    assert_eq!(err.as_deref(), Some("nope"));
    // Backoff: not ready yet.
    assert_eq!(jobs.run_pending(&pool, state(&pool)).await.unwrap(), 0);

    skip_backoff(&pool).await;
    jobs.run_pending(&pool, state(&pool)).await.unwrap();
    skip_backoff(&pool).await;
    jobs.run_pending(&pool, state(&pool)).await.unwrap();
    let (status, attempts, _) = status_of(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("dead", 3));
}

#[sqlx::test(migrations = false)]
async fn panics_are_contained(pool: PgPool) {
    setup(&pool).await;
    let id = northstack::enqueue(&pool, &Panics).await.unwrap();
    registry().run_pending(&pool, state(&pool)).await.unwrap();
    let (status, _, err) = status_of(&pool, id).await;
    assert_eq!(status, "dead");
    assert_eq!(err.as_deref(), Some("job panicked"));
}

#[sqlx::test(migrations = false)]
async fn timeouts_are_enforced(pool: PgPool) {
    setup(&pool).await;
    let id = northstack::enqueue(&pool, &Slow).await.unwrap();
    let started = std::time::Instant::now();
    registry().run_pending(&pool, state(&pool)).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    let (status, _, err) = status_of(&pool, id).await;
    assert_eq!(status, "dead");
    assert!(err.unwrap().contains("timed out"));
}

#[sqlx::test(migrations = false)]
async fn unknown_kinds_are_marked_dead(pool: PgPool) {
    setup(&pool).await;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO northstack_jobs (kind, payload, max_attempts, timeout_secs) VALUES ('nobody.handles', '{}', 5, 60) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    registry().run_pending(&pool, state(&pool)).await.unwrap();
    assert_eq!(status_of(&pool, id).await.0, "dead");
}

#[sqlx::test(migrations = false)]
async fn expired_leases_are_retried(pool: PgPool) {
    setup(&pool).await;
    let id = northstack::enqueue(&pool, &Record { n: 7 }).await.unwrap();
    // Simulate a worker that claimed the job and then crashed.
    sqlx::query(
        "UPDATE northstack_jobs SET status = 'running', attempts = 1, locked_until = now() - interval '1 second' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let s = state(&pool);
    assert_eq!(registry().run_pending(&pool, s.clone()).await.unwrap(), 1);
    let (status, attempts, _) = status_of(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("done", 2));
}

/// Several "replicas" with many concurrent workers each: every job runs
/// exactly once.
#[sqlx::test(migrations = false)]
async fn replicas_never_double_process(pool: PgPool) {
    setup(&pool).await;
    const N: i32 = 300;
    for n in 0..N {
        northstack::enqueue(&pool, &Record { n }).await.unwrap();
    }
    let s = state(&pool);
    let jobs = Arc::new(registry());
    let shutdown = CancellationToken::new();
    let mut replicas = Vec::new();
    for _ in 0..4 {
        let cfg = WorkerConfig {
            concurrency: 8,
            poll_interval: Duration::from_millis(50),
            ..Default::default()
        };
        replicas.push(tokio::spawn(northstack::jobs::run_workers(
            pool.clone(),
            jobs.clone(),
            s.clone(),
            cfg,
            shutdown.clone(),
        )));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let done: i64 =
            sqlx::query_scalar("SELECT count(*) FROM northstack_jobs WHERE status = 'done'")
                .fetch_one(&pool)
                .await
                .unwrap();
        if done == N as i64 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "only {done}/{N} jobs finished"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    shutdown.cancel();
    for r in replicas {
        r.await.unwrap();
    }
    let (runs, distinct): (i64, i64) =
        sqlx::query_as("SELECT count(*), count(DISTINCT job_id) FROM runs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((runs, distinct), (N as i64, N as i64));
    assert_eq!(s.calls.load(Ordering::SeqCst), N as usize);
}

/// With a long poll interval, NOTIFY must wake an idle worker right away.
#[sqlx::test(migrations = false)]
async fn notify_wakes_idle_workers(pool: PgPool) {
    setup(&pool).await;
    let s = state(&pool);
    let shutdown = CancellationToken::new();
    let cfg = WorkerConfig {
        concurrency: 2,
        poll_interval: Duration::from_secs(60),
        ..Default::default()
    };
    let worker = tokio::spawn(northstack::jobs::run_workers(
        pool.clone(),
        Arc::new(registry()),
        s.clone(),
        cfg,
        shutdown.clone(),
    ));
    // Let the worker go idle and start listening.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let started = std::time::Instant::now();
    let id = northstack::enqueue(&pool, &Record { n: 1 }).await.unwrap();
    loop {
        if status_of(&pool, id).await.0 == "done" {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "worker was not woken by NOTIFY"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    shutdown.cancel();
    worker.await.unwrap();
}

/// Every replica fires the same cron slot; exactly one job is enqueued.
#[sqlx::test(migrations = false)]
async fn cron_slot_enqueues_once_across_replicas(pool: PgPool) {
    setup(&pool).await;
    let make = || {
        northstack::Cron::new().add("test.tick", "0 * * * * *", |at| Record {
            n: at.timestamp() as i32,
        })
    };
    let at = chrono::Utc::now();
    let mut handles = Vec::new();
    for _ in 0..8 {
        let (pool, cron) = (pool.clone(), make());
        handles.push(tokio::spawn(async move {
            cron.fire(&pool, "test.tick", at).await.unwrap()
        }));
    }
    let mut inserted = 0;
    for h in handles {
        if h.await.unwrap() {
            inserted += 1;
        }
    }
    assert_eq!(inserted, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM northstack_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    // The next slot is a different job.
    assert!(
        make()
            .fire(&pool, "test.tick", at + chrono::Duration::minutes(1))
            .await
            .unwrap()
    );
}

#[test]
#[should_panic(expected = "invalid cron expression")]
fn invalid_cron_panics_at_startup() {
    let _ = northstack::Cron::new().add("bad", "every day", |_| Record { n: 0 });
}

#[test]
#[should_panic(expected = "registered twice")]
fn duplicate_job_kind_panics() {
    let _ = Jobs::<State>::new().register(record).register(record);
}

/// Graceful shutdown waits for running jobs to finish.
#[sqlx::test(migrations = false)]
async fn shutdown_waits_for_running_jobs(pool: PgPool) {
    setup(&pool).await;

    #[derive(Serialize, Deserialize)]
    struct Sleepy;
    impl Job for Sleepy {
        const KIND: &'static str = "test.sleepy";
    }
    let jobs = Jobs::new().register(|s: State, _j: Sleepy| async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        s.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let id = northstack::enqueue(&pool, &Sleepy).await.unwrap();
    let s = state(&pool);
    let shutdown = CancellationToken::new();
    let cfg = WorkerConfig {
        poll_interval: Duration::from_millis(20),
        ..Default::default()
    };
    let worker = tokio::spawn(northstack::jobs::run_workers(
        pool.clone(),
        Arc::new(jobs),
        s.clone(),
        cfg,
        shutdown.clone(),
    ));
    // Wait until it is running, then ask to stop.
    loop {
        if status_of(&pool, id).await.0 == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown.cancel();
    worker.await.unwrap();
    assert_eq!(s.calls.load(Ordering::SeqCst), 1);
    assert_eq!(status_of(&pool, id).await.0, "done");
}
