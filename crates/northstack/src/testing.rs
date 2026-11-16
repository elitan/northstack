//! Helpers for integration tests.
//!
//! [`db`] gives every test its own fresh, migrated SQLite file (deleted when
//! the test ends), opened with the same settings as production. Tests run in
//! parallel without sharing data:
//!
//! ```ignore
//! #[tokio::test]
//! async fn creates_note() {
//!     let db = northstack::testing::db(&server::MIGRATOR).await;
//!     let app = server::build();
//!     let client = Client::new(app.router(AppState::new(db.pool()), db.pool(), Duration::from_secs(30)));
//!     ...
//! }
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use tower::ServiceExt;

use crate::RpcError;

/// A temporary database for one test. Deleted on drop.
pub struct TestDb {
    pool: SqlitePool,
    path: PathBuf,
}

impl TestDb {
    /// The pool (cheap to clone).
    pub fn pool(&self) -> SqlitePool {
        self.pool.clone()
    }
}

impl std::ops::Deref for TestDb {
    type Target = SqlitePool;
    fn deref(&self) -> &SqlitePool {
        &self.pool
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = self.path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

/// A fresh database with the framework's and the app's migrations applied.
/// Pass `&sqlx::migrate::Migrator::DEFAULT` for framework tables only.
pub async fn db(migrator: &Migrator) -> TestDb {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "northstack-test-{}-{}-{nanos}.db",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let url = format!("sqlite:{}", path.display());
    let pool = crate::db::connect(&url, 8)
        .await
        .unwrap_or_else(|e| panic!("creating test database {}: {e:#}", path.display()));
    let db = TestDb { pool, path };
    crate::migrate(&db, migrator)
        .await
        .unwrap_or_else(|e| panic!("migrating test database: {e:#}"));
    db
}

/// In-process HTTP client for an axum router. No network involved.
#[derive(Clone)]
pub struct Client {
    router: axum::Router,
    headers: Vec<(String, String)>,
}

impl Client {
    pub fn new(router: axum::Router) -> Self {
        Self {
            router,
            headers: Vec::new(),
        }
    }

    /// Add a header to every request (e.g. authorization).
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Call a procedure and decode the typed result.
    pub async fn call<I: Serialize, O: DeserializeOwned>(
        &self,
        name: &str,
        input: &I,
    ) -> Result<O, RpcError> {
        let (status, body) = self
            .post_raw(&format!("/rpc/{name}"), serde_json::to_vec(input).unwrap())
            .await;
        if status.is_success() {
            Ok(serde_json::from_slice(&body).unwrap_or_else(|e| {
                panic!(
                    "decoding output of {name}: {e}; body: {}",
                    String::from_utf8_lossy(&body)
                )
            }))
        } else {
            Err(serde_json::from_slice(&body).unwrap_or_else(|e| {
                panic!(
                    "decoding error of {name} ({status}): {e}; body: {}",
                    String::from_utf8_lossy(&body)
                )
            }))
        }
    }

    /// POST raw bytes; returns status and body.
    pub async fn post_raw(&self, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
        let mut req = Request::post(path).header("content-type", "application/json");
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        self.send(req.body(Body::from(body)).unwrap()).await
    }

    /// GET a path; returns status and body.
    pub async fn get(&self, path: &str) -> (StatusCode, Vec<u8>) {
        let mut req = Request::get(path);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        self.send(req.body(Body::empty()).unwrap()).await
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let res = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("router is infallible");
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, body.to_vec())
    }
}
