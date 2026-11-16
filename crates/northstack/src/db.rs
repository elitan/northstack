//! The database: one SQLite file, opened the same way everywhere.
//!
//! - WAL mode: readers never block the writer and vice versa.
//! - `busy_timeout`: a writer waits up to 5 s for another writer instead of
//!   failing with "database is locked" (SQLite allows one writer at a time).
//! - Foreign keys are enforced.
//! - Start transactions with [`begin`] (`BEGIN IMMEDIATE`), not `pool.begin()`.
//!   A deferred transaction that reads and then writes fails with
//!   `SQLITE_BUSY` when another connection wrote in between; an immediate one
//!   takes the write lock up front and waits its turn instead.
//!
//! Times are stored as ISO 8601 text in UTC: [`NOW`] in SQL, or a bound
//! `chrono::DateTime<Utc>` from Rust. Both end in `+00:00`, so they compare
//! and sort correctly as text.

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

/// The current time in SQL, in the layout sqlx uses for `DateTime<Utc>`
/// (sqlx writes 0, 3, 6 or 9 fractional digits, this always 3; they still
/// sort correctly together): `strftime('%Y-%m-%dT%H:%M:%f+00:00')`. Use it
/// for defaults and updates:
///
/// ```sql
/// created_at TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
/// UPDATE notes SET updated_at = strftime('%Y-%m-%dT%H:%M:%f+00:00') WHERE ...
/// ```
pub const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%f+00:00')";

/// Connection options for `url` (`sqlite:data/app.db`, a path, or
/// `sqlite::memory:`) with northstack's settings.
pub fn options(url: &str) -> sqlx::Result<SqliteConnectOptions> {
    let url = if url.starts_with("sqlite:") {
        url.to_string()
    } else {
        format!("sqlite:{url}")
    };
    Ok(SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .optimize_on_close(true, None))
}

/// Open a pool, creating the file (and its directory) if needed.
pub async fn connect(url: &str, max_connections: u32) -> anyhow::Result<SqlitePool> {
    let options = options(url)?;
    let path = options.get_filename().to_path_buf();
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && !is_memory(url)
    {
        std::fs::create_dir_all(dir)
            .map_err(|e| anyhow::anyhow!("creating database directory {}: {e}", dir.display()))?;
    }
    SqlitePoolOptions::new()
        .max_connections(max_connections.max(1))
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(|e| anyhow::anyhow!("opening database {}: {e}", path.display()))
}

/// `sqlite::memory:` or `?mode=memory`. (sqlx names these
/// `file:sqlx-in-memory-N`, so the filename alone doesn't tell.)
fn is_memory(url: &str) -> bool {
    url.contains(":memory:") || url.contains("mode=memory")
}

/// Start a write transaction (`BEGIN IMMEDIATE`). Use this instead of
/// `pool.begin()`; see the module docs.
///
/// ```ignore
/// let mut tx = northstack::db::begin(&ctx.state.db).await?;
/// let note = db::insert(&mut *tx, &input.title).await?;
/// northstack::enqueue(&mut *tx, &AnalyzeNote { note_id: note.id }).await?;
/// tx.commit().await?;
/// ```
pub async fn begin(pool: &SqlitePool) -> sqlx::Result<Transaction<'static, Sqlite>> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

/// The file behind a `DATABASE_URL`, for messages and `db-reset`. An error
/// for an in-memory database, which has no file.
pub fn file_path(url: &str) -> sqlx::Result<PathBuf> {
    if is_memory(url) {
        return Err(sqlx::Error::Configuration(
            format!("{url} is an in-memory database; it has no file").into(),
        ));
    }
    Ok(options(url)?.get_filename().to_path_buf())
}
