//! The SQLite settings northstack relies on: concurrent writers wait instead
//! of failing, and times compare correctly as text.

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use northstack::testing::db;
use sqlx::migrate::Migrator;

static NONE: Migrator = Migrator::DEFAULT;

#[tokio::test]
async fn settings_are_applied() {
    let db = db(&NONE).await;
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(fk, 1);
    let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(timeout, 5000);
}

/// Many concurrent read-then-write transactions (the pattern that fails with
/// SQLITE_BUSY under deferred transactions) all succeed with `db::begin`.
#[tokio::test]
async fn concurrent_write_transactions_all_succeed() {
    let db = db(&NONE).await;
    sqlx::query("CREATE TABLE counter (n INTEGER NOT NULL)")
        .execute(&*db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO counter (n) VALUES (0)")
        .execute(&*db)
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for _ in 0..64 {
        let pool = db.pool();
        tasks.push(tokio::spawn(async move {
            let mut tx = northstack::db::begin(&pool).await?;
            let n: i64 = sqlx::query_scalar("SELECT n FROM counter")
                .fetch_one(&mut *tx)
                .await?;
            tokio::time::sleep(Duration::from_millis(2)).await;
            sqlx::query("UPDATE counter SET n = $1")
                .bind(n + 1)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>(())
        }));
    }
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    let n: i64 = sqlx::query_scalar("SELECT n FROM counter")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(n, 64, "no lost updates");
}

/// Times written by SQL (`NOW`) and times bound from Rust (any precision)
/// sort chronologically as text, and round-trip through `DateTime<Utc>`.
#[tokio::test]
async fn times_compare_correctly_as_text() {
    let db = db(&NONE).await;
    sqlx::query("CREATE TABLE t (label TEXT NOT NULL, at TIMESTAMP NOT NULL)")
        .execute(&*db)
        .await
        .unwrap();
    let base = Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap();
    let times: Vec<(&str, DateTime<Utc>)> = vec![
        ("whole second", base),
        ("1 ms", base + chrono::Duration::milliseconds(1)),
        ("120 ms", base + chrono::Duration::milliseconds(120)),
        ("123.456 ms", base + chrono::Duration::microseconds(123_456)),
        (
            "123.456789 ms",
            base + chrono::Duration::nanoseconds(123_456_789),
        ),
        ("500 ms", base + chrono::Duration::milliseconds(500)),
        ("next second", base + chrono::Duration::seconds(1)),
        ("next day", base + chrono::Duration::days(1)),
    ];
    for (label, at) in &times {
        sqlx::query("INSERT INTO t (label, at) VALUES ($1, $2)")
            .bind(label)
            .bind(at)
            .execute(&*db)
            .await
            .unwrap();
    }
    // SQL-written times in the same format, at fixed instants.
    for (label, text) in [
        ("sql 250 ms", "2026-11-15T10:00:00.250+00:00"),
        ("sql 0 ms", "2026-11-15T10:00:00.000+00:00"),
    ] {
        sqlx::query("INSERT INTO t (label, at) VALUES ($1, $2)")
            .bind(label)
            .bind(text)
            .execute(&*db)
            .await
            .unwrap();
    }
    let rows: Vec<(String, DateTime<Utc>)> =
        sqlx::query_as("SELECT label, at FROM t ORDER BY at, label")
            .fetch_all(&*db)
            .await
            .unwrap();
    let decoded: Vec<DateTime<Utc>> = rows.iter().map(|r| r.1).collect();
    let mut sorted = decoded.clone();
    sorted.sort();
    assert_eq!(decoded, sorted, "text order != time order: {rows:?}");
    // Round trip keeps full precision.
    let got: DateTime<Utc> = sqlx::query_scalar("SELECT at FROM t WHERE label = '123.456789 ms'")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(got, times[4].1);
    // Range queries with a bound time work against SQL-written times.
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM t WHERE at > $1")
        .bind(base + chrono::Duration::milliseconds(200))
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(
        after, 4,
        "expected 250 ms (sql), 500 ms, next second, next day: {rows:?}"
    );
    // NOW in SQL decodes as a DateTime<Utc> close to the current time.
    assert_eq!(northstack::db::NOW, "strftime('%Y-%m-%dT%H:%M:%f+00:00')");
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%f+00:00')")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert!((Utc::now() - now).num_seconds().abs() < 5);
}

#[tokio::test]
async fn test_databases_are_isolated_and_deleted() {
    let a = db(&NONE).await;
    let b = db(&NONE).await;
    sqlx::query("CREATE TABLE only_in_a (x INTEGER)")
        .execute(&*a)
        .await
        .unwrap();
    let in_b: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE name = 'only_in_a'")
            .fetch_one(&*b)
            .await
            .unwrap();
    assert_eq!(in_b, 0);
    let path = a.connect_options().get_filename().to_path_buf();
    assert!(path.exists());
    drop(a);
    assert!(!path.exists(), "test database not deleted");
}

/// `db-reset` deletes the file behind DATABASE_URL, so an in-memory URL must
/// be refused rather than mapped to sqlx's internal `file:sqlx-in-memory-N`
/// name (a relative path that could exist on disk).
#[tokio::test]
async fn file_path_is_the_file_and_refuses_memory() {
    use northstack::db::file_path;
    assert_eq!(
        file_path("sqlite:data/dev.db").unwrap(),
        std::path::Path::new("data/dev.db")
    );
    assert_eq!(
        file_path("data/dev.db").unwrap(),
        std::path::Path::new("data/dev.db")
    );
    assert!(file_path("sqlite::memory:").is_err());
    assert!(file_path("sqlite://?mode=memory").is_err());
    // And an in-memory pool opens without touching the disk.
    let pool = northstack::db::connect("sqlite::memory:", 1).await.unwrap();
    let one: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(one, 1);
}

/// The table-rebuild migration documented in README ("Database" ->
/// "Migrations") keeps rows in tables that reference the rebuilt one.
#[tokio::test]
async fn documented_table_rebuild_keeps_referencing_rows() {
    let dir = std::env::temp_dir().join(format!(
        "northstack-rebuild-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("20990101000001_init.sql"),
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT NOT NULL);
         CREATE TABLE comments (id INTEGER PRIMARY KEY,
             note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE, body TEXT NOT NULL);
         INSERT INTO notes (id, title) VALUES (1, 'a'), (2, 'b');
         INSERT INTO comments (note_id, body) VALUES (1, 'x'), (1, 'y'), (2, 'z');",
    )
    .unwrap();
    std::fs::write(
        dir.join("20990101000002_rebuild.sql"),
        "-- no-transaction
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE notes_new (id INTEGER PRIMARY KEY, title TEXT NOT NULL CHECK (length(title) <= 100));
INSERT INTO notes_new (id, title) SELECT id, title FROM notes;
DROP TABLE notes;
ALTER TABLE notes_new RENAME TO notes;
COMMIT;
PRAGMA foreign_keys = ON;
",
    )
    .unwrap();
    let migrator = Migrator::new(dir.clone()).await.unwrap();
    let db = db(&migrator).await;
    let _ = std::fs::remove_dir_all(&dir);

    let comments: i64 = sqlx::query_scalar("SELECT count(*) FROM comments")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(comments, 3, "rebuilding notes must not delete comments");
    // The new constraint holds, the reference follows the rename, and
    // foreign keys are on again (the cascade still works).
    let too_long = sqlx::query("INSERT INTO notes (title) VALUES (printf('%.200c', 'x'))")
        .execute(&*db)
        .await;
    assert!(too_long.is_err());
    sqlx::query("DELETE FROM notes WHERE id = 2")
        .execute(&*db)
        .await
        .unwrap();
    let comments: i64 = sqlx::query_scalar("SELECT count(*) FROM comments")
        .fetch_one(&*db)
        .await
        .unwrap();
    assert_eq!(
        comments, 2,
        "ON DELETE CASCADE still applies after the rebuild"
    );
}

/// SQLite is built for concurrency (see .cargo/config.toml): no shared page
/// cache mutex, no memory-statistics mutex. Without these, concurrent queries
/// take turns and throughput drops by 10x under load.
#[tokio::test]
async fn sqlite_is_built_for_concurrent_readers() {
    let db = db(&NONE).await;
    let opts: Vec<String> =
        sqlx::query_scalar("SELECT compile_options FROM pragma_compile_options")
            .fetch_all(&*db)
            .await
            .unwrap();
    assert!(
        !opts.iter().any(|o| o == "ENABLE_MEMORY_MANAGEMENT"),
        "SQLite was built with ENABLE_MEMORY_MANAGEMENT; is LIBSQLITE3_FLAGS from .cargo/config.toml applied? {opts:?}"
    );
    assert!(
        opts.iter().any(|o| o == "DEFAULT_MEMSTATUS=0"),
        "SQLite was built without DEFAULT_MEMSTATUS=0; is LIBSQLITE3_FLAGS from .cargo/config.toml applied? {opts:?}"
    );
}
