//! A dedicated thread pool for CPU-heavy work (parsing, PDF/image generation,
//! big calculations), sized to leave one core free for serving requests.
//!
//! ```ignore
//! let report = northstack::cpu::spawn(move || build_report(&rows)).await?;
//! ```
//!
//! Never do heavy CPU work directly in an async handler or job: it blocks the
//! runtime threads that serve HTTP requests.

use std::sync::OnceLock;

static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

fn pool() -> &'static rayon::ThreadPool {
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let threads = std::env::var("CPU_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| cores.saturating_sub(1).max(1));
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("northstack-cpu-{i}"))
            .build()
            .expect("failed to build CPU thread pool")
    })
}

/// Number of threads in the CPU pool.
pub fn threads() -> usize {
    pool().current_num_threads()
}

/// Run `f` on the CPU pool and await its result. A panic inside `f` is
/// returned as an error instead of crashing the process.
pub async fn spawn<F, R>(f: F) -> anyhow::Result<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    pool().spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = tx.send(result);
    });
    match rx.await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(anyhow::anyhow!("CPU task panicked")),
        Err(_) => Err(anyhow::anyhow!("CPU task was dropped")),
    }
}

/// Run `f` on the CPU pool with access to rayon's parallel iterators, e.g.
/// `northstack::cpu::spawn(|| rows.par_iter().map(score).collect::<Vec<_>>())`.
pub use rayon::prelude;

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn runs_and_contains_panics() {
        assert_eq!(super::spawn(|| 2 + 2).await.unwrap(), 4);
        assert!(super::spawn(|| panic!("boom")).await.is_err());
    }
}
